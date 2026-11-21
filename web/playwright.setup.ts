import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  buildBackend,
  dropRegisteredDatabases,
  maintenanceUrl,
  sql,
  stackEnvironment,
  templateDatabase,
} from './e2e/stack';

// Runs once per Playwright run. Each worker starts its own disposable stack
// from the backend built here; see e2e/stack.ts and the fixture in e2e/test.ts.
export default async function setup() {
  templateDatabase();
  await sql(maintenanceUrl(), 'SELECT 1');
  const temporary = await mkdtemp(join(tmpdir(), 'neondojo-e2e-run-'));
  const registry = join(temporary, 'databases');
  await writeFile(registry, '');
  const cleanup = async () => {
    try {
      await dropRegisteredDatabases(registry);
    } finally {
      await rm(temporary, { recursive: true, force: true });
    }
  };
  try {
    process.env[stackEnvironment.backend] = await buildBackend();
    process.env[stackEnvironment.registry] = registry;
    return cleanup;
  } catch (error) {
    await cleanup();
    throw error;
  }
}
