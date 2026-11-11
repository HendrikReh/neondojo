import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';

test('browser imports reach the disposable PostgreSQL database', async ({
  request,
}) => {
  const database = process.env.BGTUTOR_E2E_DATABASE_URL;
  expect(database, 'Playwright must provision a test database').toBeDefined();
  expect(new URL(database!).pathname).toMatch(/^\/bgtutor_e2e_[a-f0-9]+$/);

  const player = `E2E isolation ${randomUUID()}`;
  const imported = await request.post('/api/imports', {
    multipart: {
      file: {
        name: 'database-isolation.sgf',
        mimeType: 'application/octet-stream',
        buffer: Buffer.from(
          `(;GM[6]PW[${player}]PB[Opponent]MI[length:7];W[31hefe])`,
        ),
      },
    },
  });
  expect(imported.status()).toBe(201);

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
