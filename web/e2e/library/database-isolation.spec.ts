import { importSgfMatch } from '../helpers/match-fixtures';
import { expect, backendTest as test } from '../helpers/test';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';

test('browser imports reach the disposable PostgreSQL database', async ({
  request,
}) => {
  const database = process.env.NEONDOJO_E2E_DATABASE_URL;
  expect(database, 'Playwright must provision a test database').toBeDefined();
  expect(new URL(database!).pathname).toMatch(/^\/neondojo_e2e_[a-f0-9]+$/);

  const player = `E2E isolation ${randomUUID()}`;
  await importSgfMatch(request, {
    filename: 'database-isolation.sgf',
    game: { white: player, moves: ';W[31hefe]' },
  });

  const owner = execFileSync(
    'psql',
    [
      database!,
      '-XAt',
      '-v',
      'ON_ERROR_STOP=1',
      '-c',
      `SELECT d.name FROM matches m JOIN databases d ON d.id=m.database_id WHERE m.white='${player}'`,
    ],
    { encoding: 'utf8' },
  );
  expect(owner.trim()).toBe('Unsorted');
});
