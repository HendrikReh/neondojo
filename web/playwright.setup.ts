import { execFile, spawn, type ChildProcess } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { createServer, type ViteDevServer } from 'vite';

const exec = promisify(execFile);
const root = fileURLToPath(new URL('../', import.meta.url));
const web = join(root, 'web');
const localPostgres =
  'postgres://bgtutor:bgtutor_local@127.0.0.1:54329/postgres';

async function sql(url: URL, statement: string) {
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

function listeningUrl(child: ChildProcess) {
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
      const bound = /BG Tutor listening[^\n]*127\.0\.0\.1:(\d+)/.exec(log);
      if (bound) finish(undefined, `http://127.0.0.1:${bound[1]}`);
    };
    function finish(error?: Error, url?: string) {
      clearTimeout(timer);
      child.removeListener('error', finish);
      child.removeListener('exit', exited);
      child.stdout?.off('data', output);
      child.stderr?.off('data', output);
      // Keep draining the pipes after startup.
      child.stdout?.resume();
      child.stderr?.resume();
      if (error) reject(error);
      else resolve(url!);
    }
    child.once('error', finish);
    child.once('exit', exited);
    child.stdout?.on('data', output);
    child.stderr?.on('data', output);
  });
}

export default async function setup() {
  const maintenance = new URL(
    process.env.BGTUTOR_E2E_POSTGRES_URL ?? localPostgres,
  );
  const template = process.env.BGTUTOR_E2E_POSTGRES_TEMPLATE ?? 'template0';
  if (
    template !== 'template0' &&
    !/^bgtutor_e2e_template_[a-z0-9_]+$/.test(template)
  )
    throw new Error(
      'Use a bgtutor_e2e_template_* PostgreSQL fixture database.',
    );
  const name = `bgtutor_e2e_${randomUUID().replaceAll('-', '')}`;
  const database = new URL(maintenance);
  database.pathname = `/${name}`;
  database.searchParams.delete('dbname');
  const temporary = await mkdtemp(join(tmpdir(), 'bgtutor-e2e-'));
  let created = false;
  let backend: ChildProcess | undefined;
  let frontend: ViteDevServer | undefined;
  const cleanup = async () => {
    try {
      await frontend?.close();
    } finally {
      try {
        if (backend) await stopBackend(backend);
        if (created)
          await sql(maintenance, `DROP DATABASE "${name}" WITH (FORCE)`);
      } finally {
        await rm(temporary, { recursive: true, force: true });
      }
    }
  };

  try {
    await sql(maintenance, `CREATE DATABASE "${name}" TEMPLATE "${template}"`);
    created = true;
    if ((await sql(database, 'SELECT current_database()')) !== name)
      throw new Error('PostgreSQL URL did not select the new test database.');

    await exec('npm', ['run', 'themes:prepare'], { cwd: web });
    const { stdout } = await exec(
      'cargo',
      ['build', '-p', 'bgtutor', '--bin', 'bgtutor', '--message-format=json'],
      { cwd: root, maxBuffer: 8 * 1024 * 1024 },
    );
    const executable = stdout
      .trim()
      .split('\n')
      .map((line) => JSON.parse(line))
      .find(
        (message) => message.target?.name === 'bgtutor' && message.executable,
      )?.executable;
    if (!executable)
      throw new Error('Cargo did not build the BG Tutor backend.');

    backend = spawn(executable, [], {
      cwd: root,
      env: {
        ...process.env,
        DATABASE_URL: database.href,
        BGTUTOR_E2E_DATABASE_NAME: name,
        BGTUTOR_BIND: '127.0.0.1:0',
        RUST_LOG: 'bgtutor=info',
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    const backendUrl = await listeningUrl(backend);
    frontend = await createServer({
      root: web,
      cacheDir: join(temporary, 'vite'),
      server: {
        host: '127.0.0.1',
        port: 41900,
        strictPort: false,
        hmr: false,
        watch: null,
        proxy: { '/api': backendUrl },
      },
    });
    await frontend.listen();
    const url = frontend.resolvedUrls?.local[0];
    if (!url) throw new Error('Vite did not publish a local test URL.');

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

    // Playwright reads this environment variable for its built-in baseURL.
    process.env.PLAYWRIGHT_TEST_BASE_URL = url;
    process.env.BGTUTOR_E2E_DATABASE_URL = database.href;
    return cleanup;
  } catch (error) {
    await cleanup();
    throw error;
  }
}
