import { copyFile, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from './test';

const sgf = (player: string) =>
  `(;GM[6]PW[${player}]PB[Opponent]MI[length:5][ws:0][bs:0];W[31hefe];B[42qusu])`;

let folder: string;

test.beforeEach(async ({ page }) => {
  folder = await mkdtemp(join(tmpdir(), 'neondojo-folder-import-'));
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.setViewportSize({ width: 1800, height: 1169 });
});

test.afterEach(async () => {
  await rm(folder, { recursive: true, force: true });
});

async function libraryDatabase(page: import('@playwright/test').Page) {
  const response = await page.request.post('/api/databases', {
    data: { name: `Folder import ${crypto.randomUUID()}`, description: '' },
  });
  expect(response.status()).toBe(201);
  const database: { id: string; name: string } = await response.json();
  await page.addInitScript((id) => {
    localStorage.setItem('neondojo.database', id);
  }, database.id);
  return database;
}

test('a chosen folder imports every supported match file, including subfolders', async ({
  page,
}) => {
  const database = await libraryDatabase(page);
  await mkdir(join(folder, 'Round 2'));
  await writeFile(
    join(folder, 'first.sgf'),
    sgf(`First ${crypto.randomUUID()}`),
  );
  await writeFile(
    join(folder, 'Round 2', 'second.SGF'),
    sgf(`Second ${crypto.randomUUID()}`),
  );
  await copyFile(
    '../backend/tests/fixtures/hedgehog-cdf.ogxm',
    join(folder, 'Round 2', 'hedgehog.ogxm'),
  );
  // Not a match transcript: imported as MAT and reported as failed.
  await writeFile(join(folder, 'readme.txt'), 'Tournament notes');
  // Skipped before upload.
  await writeFile(join(folder, '._first.sgf'), 'AppleDouble metadata');
  await writeFile(join(folder, 'flyer.pdf'), '%PDF-1.4');

  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database' }).click();
  const uploads: string[] = [];
  page.on('request', (request) => {
    if (new URL(request.url()).pathname === '/api/imports')
      uploads.push(request.url());
  });
  await page.getByLabel('Match folder', { exact: true }).setInputFiles(folder);

  const panel = page.locator('.import-results');
  await expect(panel.locator('.import-summary').first()).toContainText(
    'Import finished: 4 files. Imported: 3 · Duplicates: 0 · Failed: 1',
  );
  await expect(panel).toContainText(
    '2 files without a supported match format skipped',
  );
  const rows = panel.locator('ul > li');
  await expect(rows).toHaveCount(4);
  const folderName = folder.split('/').pop();
  await expect(rows.nth(0)).toHaveText(`${folderName}/first.sgf: Imported`);
  await expect(rows.nth(3)).toContainText(`${folderName}/Round 2/second.SGF`);
  await expect(panel.locator('li[data-status="error"]')).toContainText(
    'readme.txt',
  );
  expect(uploads).toHaveLength(4);
  for (const url of uploads)
    expect(new URL(url).searchParams.get('database_id')).toBe(database.id);
  await expect(page.locator('.database-table tbody tr')).toHaveCount(3);

  // The same folder again reports duplicates instead of new matches.
  await page.getByLabel('Match folder', { exact: true }).setInputFiles(folder);
  await expect(panel.locator('.import-summary').first()).toContainText(
    'Imported: 0 · Duplicates: 3 · Failed: 1',
  );
  await expect(page.locator('.database-table tbody tr')).toHaveCount(3);
});

test('stopping a folder import finishes the current file and skips the rest', async ({
  page,
}) => {
  await libraryDatabase(page);
  for (const name of ['a.sgf', 'b.sgf', 'c.sgf'])
    await writeFile(join(folder, name), sgf(`Stop ${crypto.randomUUID()}`));
  let release!: () => void;
  const held = new Promise<void>((resolve) => (release = resolve));
  await page.route('**/api/imports*', async (route) => {
    await held;
    await route.continue();
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database' }).click();
  await page.getByLabel('Match folder', { exact: true }).setInputFiles(folder);
  const panel = page.locator('.import-results');
  await expect(panel.locator('progress')).toBeVisible();
  await expect(panel).toContainText('0 of 3 files processed.');
  await expect(
    page
      .locator('.library-import')
      .getByRole('button', { name: 'Import folder' }),
  ).toBeDisabled();
  await panel.getByRole('button', { name: 'Stop import' }).click();
  await expect(
    panel.getByRole('button', { name: 'Stopping after the current file …' }),
  ).toBeDisabled();
  release();
  await expect(panel.locator('.import-summary').first()).toContainText(
    'Import stopped after 1 of 3 files. Imported: 1 · Duplicates: 0 · Failed: 0',
  );
  await expect(panel.locator('ul > li')).toHaveCount(1);
  await expect(page.locator('.database-table tbody tr')).toHaveCount(1);
  await expect(
    page
      .locator('.library-import')
      .getByRole('button', { name: 'Import folder' }),
  ).toBeEnabled();
});

test('a folder without supported match files reports that nothing was imported', async ({
  page,
}) => {
  await libraryDatabase(page);
  await writeFile(join(folder, 'notes.pdf'), '%PDF-1.4');
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database' }).click();
  await page.getByLabel('Match folder', { exact: true }).setInputFiles(folder);
  const panel = page.locator('.import-results');
  await expect(panel).toContainText(
    'The folder contains no supported match files.',
  );
  await expect(panel).toContainText(
    '1 file without a supported match format skipped',
  );
  await expect(panel.locator('ul')).toHaveCount(0);
});
