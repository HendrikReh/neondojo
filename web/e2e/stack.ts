// Disposable NeonDojo stacks for browser tests. Global setup builds the backend
// once, and each Playwright worker then starts its own PostgreSQL database,
// backend process, and Vite middleware server. Workers therefore never share
// persisted data, and spec files can run in parallel.
import { execFile, spawn, type ChildProcess } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { once } from 'node:events';
import { createWriteStream } from 'node:fs';
import { appendFile, mkdtemp, readFile, rm } from 'node:fs/promises';
import { createServer as createHttpServer, type Server } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { createServer, type ViteDevServer } from 'vite';

const exec = promisify(execFile);
export const root = fileURLToPath(new URL('../../', import.meta.url));
export const web = join(root, 'web');
const localPostgres =
  'postgres://neondojo:neondojo_local@127.0.0.1:54329/postgres';

/** Set by global setup and inherited by every worker process. */
export const stackEnvironment = {
  backend: 'NEONDOJO_E2E_BACKEND_BIN',
  registry: 'NEONDOJO_E2E_DATABASE_REGISTRY',
} as const;

export interface Stack {
  url: string;
  databaseUrl: string;
  stop(): Promise<void>;
}

export function maintenanceUrl() {
  return new URL(process.env.NEONDOJO_E2E_POSTGRES_URL ?? localPostgres);
}

export function templateDatabase() {
  const template = process.env.NEONDOJO_E2E_POSTGRES_TEMPLATE ?? 'template0';
  if (
    template !== 'template0' &&
    !/^neondojo_e2e_template_[a-z0-9_]+$/.test(template)
  )
    throw new Error(
      'Use a neondojo_e2e_template_* PostgreSQL fixture database.',
    );
  return template;
}

export async function sql(url: URL, statement: string) {
  try {
    const { stdout } = await exec('psql', [
      url.href,
      '-XAt',
      '-v',
      'ON_ERROR_STOP=1',
      '-c',
      statement,
    ]);
    return stdout.trim();
  } catch (error) {
    const failure = error as { code?: string; stderr?: string };
    throw new Error(
      failure.code === 'ENOENT'
        ? 'Browser tests require the psql command.'
        : `Browser test PostgreSQL command failed. ${failure.stderr?.trim() ?? ''}`,
    );
  }
}

/** Builds the backend once per run and returns the executable path. */
export async function buildBackend() {
  await exec('npm', ['run', 'themes:prepare'], { cwd: web });
  const { stdout } = await exec(
    'cargo',
    ['build', '-p', 'neondojo', '--bin', 'neondojo', '--message-format=json'],
    { cwd: root, maxBuffer: 8 * 1024 * 1024 },
  );
  const executable = stdout
    .trim()
    .split('\n')
    .map((line) => JSON.parse(line))
    .find(
      (message) => message.target?.name === 'neondojo' && message.executable,
    )?.executable;
  if (!executable) throw new Error('Cargo did not build the NeonDojo backend.');
  return executable as string;
}

/** Drops every database a worker registered but could not remove itself. */
export async function dropRegisteredDatabases(registry: string) {
  const names = await readFile(registry, 'utf8').catch(() => '');
  const maintenance = maintenanceUrl();
  for (const name of new Set(names.split('\n').filter(Boolean)))
    if (/^neondojo_e2e_[a-f0-9]{32}$/.test(name))
      await sql(maintenance, `DROP DATABASE IF EXISTS "${name}" WITH (FORCE)`);
}

async function createDatabase(maintenance: URL, name: string) {
  const template = templateDatabase();
  // PostgreSQL refuses to copy a template while another session uses it, and
  // parallel workers copy the same template during startup.
  for (let attempt = 0; ; attempt++) {
    try {
      await sql(
        maintenance,
        `CREATE DATABASE "${name}" TEMPLATE "${template}"`,
      );
      return;
    } catch (error) {
      if (
        attempt >= 20 ||
        !String(error).includes('is being accessed by other users')
      )
        throw error;
      await new Promise((resolve) =>
        setTimeout(resolve, 100 + Math.random() * 200),
      );
    }
  }
}

async function stopBackend(child: ChildProcess) {
  if (!child.pid || child.exitCode !== null || child.signalCode !== null)
    return;
  const exited = once(child, 'exit');
  const force = setTimeout(() => child.kill('SIGKILL'), 5_000);
  try {
    child.kill('SIGTERM');
    await exited;
  } finally {
    clearTimeout(force);
  }
}

function listeningUrl(child: ChildProcess, logFile: string | undefined) {
  return new Promise<string>((resolve, reject) => {
    let log = '';
    const timer = setTimeout(
      () => finish(new Error(`Browser test backend did not start.\n${log}`)),
      45_000,
    );
    const exited = () =>
      finish(new Error(`Browser test backend exited during startup.\n${log}`));
    const output = (chunk: Buffer) => {
      log = (log + chunk.toString()).slice(-12_000);
      const bound = /NeonDojo listening[^\n]*127\.0\.0\.1:(\d+)/.exec(log);
      if (bound) finish(undefined, `http://127.0.0.1:${bound[1]}`);
    };
    function finish(error?: Error, url?: string) {
      clearTimeout(timer);
      child.removeListener('error', finish);
      child.removeListener('exit', exited);
      child.stdout?.off('data', output);
      child.stderr?.off('data', output);
      // Keep draining the pipes after startup, optionally into a log file.
      const sink = logFile
        ? createWriteStream(logFile, { flags: 'a' })
        : undefined;
      if (sink) {
        sink.write(log);
        child.stdout?.pipe(sink, { end: false });
        child.stderr?.pipe(sink, { end: false });
      } else {
        child.stdout?.resume();
        child.stderr?.resume();
      }
      if (error) reject(error);
      else resolve(url!);
    }
    child.once('error', finish);
    child.once('exit', exited);
    child.stdout?.on('data', output);
    child.stderr?.on('data', output);
  });
}

/**
 * NEONDOJO_E2E_BACKEND_LOG names a file for the backend output. With more than
 * one worker, each worker appends its index to the file name.
 */
function backendLog(worker: number) {
  const file = process.env.NEONDOJO_E2E_BACKEND_LOG;
  if (!file || worker === 0) return file;
  return file.replace(/(\.[^./]+)?$/, `-${worker}$1`);
}

export async function startStack(worker: number): Promise<Stack> {
  const executable = process.env[stackEnvironment.backend];
  const registry = process.env[stackEnvironment.registry];
  if (!executable || !registry)
    throw new Error('Browser test global setup did not run.');
  const maintenance = maintenanceUrl();
  const name = `neondojo_e2e_${randomUUID().replaceAll('-', '')}`;
  const database = new URL(maintenance);
  database.pathname = `/${name}`;
  database.searchParams.delete('dbname');
  const temporary = await mkdtemp(join(tmpdir(), 'neondojo-e2e-'));
  let created = false;
  let backend: ChildProcess | undefined;
  let frontend: ViteDevServer | undefined;
  let listener: Server | undefined;
  const stop = async () => {
    try {
      try {
        await frontend?.close();
      } finally {
        if (listener?.listening) {
          const closed = once(listener, 'close');
          listener.close();
          listener.closeAllConnections();
          await closed;
        }
      }
    } finally {
      try {
        if (backend) await stopBackend(backend);
        if (created)
          await sql(
            maintenance,
            `DROP DATABASE IF EXISTS "${name}" WITH (FORCE)`,
          );
      } finally {
        await rm(temporary, { recursive: true, force: true });
      }
    }
  };

  try {
    // Register first so global teardown can drop the database after a crash.
    await appendFile(registry, `${name}\n`);
    await createDatabase(maintenance, name);
    created = true;
    if ((await sql(database, 'SELECT current_database()')) !== name)
      throw new Error('PostgreSQL URL did not select the new test database.');

    backend = spawn(executable, [], {
      cwd: root,
      env: {
        ...process.env,
        DATABASE_URL: database.href,
        NEONDOJO_E2E_DATABASE_NAME: name,
        NEONDOJO_BIND: '127.0.0.1:0',
        // Parallel test backends share one PostgreSQL server; keep each pool
        // small so they stay below its connection limit.
        NEONDOJO_DB_MAX_CONNECTIONS: '10',
        RUST_LOG: 'neondojo=info',
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const backendUrl = await listeningUrl(backend, backendLog(worker));
    const http = createHttpServer();
    listener = http;
    frontend = await createServer({
      root: web,
      cacheDir: join(temporary, 'vite'),
      logLevel: 'warn',
      server: {
        middlewareMode: true,
        // Vite injects its dev client in middleware mode. Serve its WebSocket
        // on the same loopback listener as the browser tests.
        hmr: { server: http },
        // Vite discards null options when merging the project's config.
        watch: { ignored: () => true },
        proxy: { '/api': backendUrl },
      },
    });
    listener.on('request', frontend.middlewares);
    const listening = once(listener, 'listening');
    listener.listen(0, '127.0.0.1');
    await listening;
    const address = listener.address();
    if (!address || typeof address === 'string')
      throw new Error('Vite did not publish a local test URL.');
    const url = `http://127.0.0.1:${address.port}/`;

    // Verify the proxy reaches this database before allowing any test writes.
    const probe = randomUUID();
    await sql(
      database,
      `INSERT INTO databases (id, name) VALUES ('${probe}', 'Playwright connection check')`,
    );
    const response = await fetch(new URL('/api/databases', url), {
      signal: AbortSignal.timeout(10_000),
    });
    const databases = response.ok ? await response.json() : null;
    if (
      !Array.isArray(databases) ||
      !databases.some((entry) => entry.id === probe)
    )
      throw new Error('Vite API requests did not reach the test database.');
    await sql(database, `DELETE FROM databases WHERE id = '${probe}'`);
    return { url, databaseUrl: database.href, stop };
  } catch (error) {
    await stop();
    throw error;
  }
}
