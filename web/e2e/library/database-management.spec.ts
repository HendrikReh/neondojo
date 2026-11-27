import { readFile } from 'node:fs/promises';
import type { Database } from '../../src/workspaces/library/library-api';
import {
  createDatabase,
  waitForJobCompletion,
} from '../helpers/match-fixtures';
import {
  gotoWorkspace,
  openWorkspace,
  seedLocale,
  seedStorage,
} from '../helpers/navigation';
import { expect, test } from '../helpers/test';

test('import target can differ from the active library collection', async ({
  page,
}) => {
  const id = crypto.randomUUID();
  const source = await createDatabase(page, `Import browsing ${id}`);
  const target = await createDatabase(page, `Import destination ${id}`);
  await seedStorage(page, { locale: 'en', databaseId: source.id });
  await gotoWorkspace(page, 'library');
  const collection = page
    .locator('.database-collections')
    .getByRole('button', { name: new RegExp(source.name) });
  const destination = page.getByRole('combobox', {
    name: 'Import into',
    exact: true,
  });
  await expect(page.locator('.library-import-destination')).toContainText(
    'Into:',
  );
  await expect(destination).toHaveValue(source.id);
  await expect(
    destination.getByRole('option', { name: 'Paused Matches', exact: true }),
  ).toHaveCount(0);
  await destination.selectOption(target.id);
  await expect(collection).toHaveAttribute('aria-pressed', 'true');
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: /Import match/ }).click();
  const importResponses = [1, 2].map((index) =>
    page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === '/api/imports' &&
        response.request().method() === 'POST' &&
        (response
          .request()
          .postData()
          ?.includes(`selected-destination-${index}.sgf`) ??
          false),
    ),
  );
  await (
    await chooser
  ).setFiles(
    [1, 2].map((index) => ({
      name: `selected-destination-${index}.sgf`,
      mimeType: 'application/octet-stream',
      buffer: Buffer.from(
        `(;GM[6]PW[Import ${index} ${id}]PB[Opponent]MI[length:7];W[31hefe])`,
      ),
    })),
  );
  for (const response of await Promise.all(importResponses)) {
    expect(response.status()).toBe(201);
    expect(new URL(response.url()).searchParams.get('database_id')).toBe(
      target.id,
    );
  }
  await expect(
    page.getByRole('tab', { name: 'Match database', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(collection).toHaveAttribute('aria-pressed', 'true');
  await expect(destination).toHaveValue(target.id);
  const imported = await page.request.get(
    `/api/library/matches?database_id=${target.id}`,
  );
  expect((await imported.json()).items).toHaveLength(2);
});

test('system databases keep their names and can be emptied', async ({
  page,
}) => {
  await seedLocale(page);
  await page.setViewportSize({ width: 1800, height: 1169 });
  const response = await page.request.get('/api/databases');
  expect(response.ok()).toBeTruthy();
  const databases: Database[] = await response.json();
  for (const name of ['Unsorted', 'Bot Matches']) {
    const database = databases.find((entry) => entry.name === name);
    if (!database) throw new Error(`Missing system database: ${name}`);
    expect(database.is_system).toBe(true);
    const player = `System fixture ${crypto.randomUUID()}`;
    const path = database.is_default
      ? '/api/imports'
      : `/api/imports?database_id=${database.id}`;
    const imported = await page.request.post(path, {
      multipart: {
        file: {
          name: 'system.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            `(;GM[6]PW[${player}]PB[Opponent]MI[length:7];W[31hefe])`,
          ),
        },
      },
    });
    expect(imported.status()).toBe(201);

    await gotoWorkspace(page, 'library');
    const sidebar = page.locator('.database-sidebar');
    const collection = sidebar.getByRole('button', { name: new RegExp(name) });
    await collection.click();
    await expect(page.locator('.database-table')).toContainText(player);
    await expect(
      sidebar.getByRole('button', { name: 'Delete database' }),
    ).toHaveCount(0);
    await sidebar
      .getByRole('button', { name: 'Edit database', exact: true })
      .click();
    const dialog = page.getByRole('dialog');
    const input = dialog.getByLabel('Name', { exact: true });
    await expect(input).toHaveValue(name);
    await expect(input).toHaveJSProperty('readOnly', true);
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();

    const empty = sidebar.getByRole('button', {
      name: 'Empty database',
      exact: true,
    });
    await empty.click();
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(page.locator('.database-table')).toContainText(player);
    await empty.click();
    await dialog
      .getByRole('button', { name: 'Empty database', exact: true })
      .click();
    await expect(dialog).toHaveCount(0);
    await expect(collection).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('.database-table tbody tr')).toHaveCount(0);
    await expect(empty).toBeDisabled();
  }
});

test('collections, independent copies, filters and metadata work together', async ({
  page,
}) => {
  await seedLocale(page);
  await gotoWorkspace(page, 'library');
  await page.getByRole('button', { name: 'New database', exact: true }).click();
  const name = `Library ${crypto.randomUUID()}`;
  await page.getByRole('dialog').getByLabel('Name', { exact: true }).fill(name);
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Save', exact: true })
    .click();
  const collection = page
    .locator('.database-collections')
    .getByRole('button', { name: new RegExp(name) });
  const icon = collection.locator('[data-action-glyph="database"]');
  await expect(icon).toBeVisible();
  await expect(icon).toHaveCSS('background-color', 'rgba(0, 0, 0, 0)');
  const whitePlayer = `Library ${crypto.randomUUID()}`;
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'library.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[${whitePlayer}]PB[Opponent]DT[2026-11-08]EV[Test Open]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await openWorkspace(page, 'library');
  await expect(page.locator('.database-table tbody tr')).toHaveCount(1);
  await expect(page.locator('.database-table')).toContainText(whitePlayer);
  await expect(page.locator('.database-table')).toContainText('Opponent');
  await expect(page.locator('.database-table')).not.toContainText(
    'library.sgf',
  );
  await expect(
    page
      .locator('.database-table')
      .getByRole('columnheader', { name: 'Matches', exact: true }),
  ).toHaveCount(0);
  await page.locator('.database-table tbody tr').first().click();
  await expect(page.locator('.match-preview')).toHaveCount(0);
  await page
    .locator('.database-table tbody tr')
    .first()
    .getByRole('checkbox')
    .check();
  await page
    .locator('.library-batch-bar')
    .getByRole('button', {
      name: 'Edit match details',
      exact: true,
    })
    .click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Name', { exact: true }).fill('Final training match');
  await dialog.getByLabel('Event', { exact: true }).fill('Autumn Open');
  await dialog.getByLabel('Result', { exact: true }).selectOption('completed');
  await dialog.getByLabel('White · Score', { exact: true }).fill('7');
  await dialog.getByLabel('Black · Score', { exact: true }).fill('2');
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(page.locator('.database-table')).toContainText(
    'Final training match',
  );
  await page.locator('.database-table tbody tr').first().focus();
  await page.keyboard.press('Enter');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await openWorkspace(page, 'library');
  await page
    .locator('.database-table tbody tr')
    .first()
    .locator('td')
    .nth(3)
    .dblclick();
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await openWorkspace(page, 'library');
  await page.getByRole('button', { name: 'Filters', exact: true }).click();
  await page
    .locator('.library-filters')
    .getByLabel('Result', { exact: true })
    .selectOption('black_won');
  await expect(page.locator('.database-table tbody tr')).toHaveCount(0);
  await page
    .locator('.library-filters')
    .getByLabel('Result', { exact: true })
    .selectOption('white_won');
  await expect(page.locator('.database-table tbody tr')).toHaveCount(1);
  await page
    .locator('.library-filters')
    .getByLabel('Event', { exact: true })
    .fill('not-here');
  await expect(page.locator('.database-table tbody tr')).toHaveCount(0);
  await page
    .getByRole('button', { name: 'Clear filters', exact: true })
    .click();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(1);
  await page.getByLabel('Select all loaded matches', { exact: true }).check();
  await expect(page.locator('.library-batch-bar')).toBeVisible();
  await page.getByRole('button', { name: 'Copy', exact: true }).click();
  await page
    .getByRole('dialog')
    .getByLabel('Target database')
    .selectOption('00000000-0000-0000-0000-000000000001');
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Copy', exact: true })
    .click();
  await expect(page.locator('.library-action-result')).toBeVisible();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(1);
  await page
    .getByRole('button', { name: 'Delete database', exact: true })
    .click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await openWorkspace(page, 'analysis');
  await expect(page.locator('.analysis-welcome')).toBeVisible();
});

test('a game and an explicit cube action have separate searchable notes', async ({
  page,
}) => {
  await seedLocale(page);
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'cube-notes.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Cube ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[double];W[take];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  await page.getByRole('tab', { name: 'Game', exact: true }).click();
  await expect(page.locator('[data-annotation-kind=game]')).toBeVisible();
  await page
    .locator('[data-annotation-kind=game]')
    .getByLabel('Comment', { exact: true })
    .fill('Game strategy notes');
  await page
    .locator('[data-annotation-kind=game]')
    .getByRole('button', { name: 'Save notes', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Cube and move notes', exact: true })
    .click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Move', { exact: true }).selectOption({ index: 1 });
  await expect(dialog.locator('[data-annotation-kind=move]')).toBeVisible();
  await dialog.getByLabel('Move', { exact: true }).selectOption({ index: 2 });
  await expect(dialog.locator('[data-cube-pending=true]')).toHaveAttribute(
    'data-cube',
    '2',
  );
  await expect(dialog.locator('[data-cube-pending=true]')).toHaveAttribute(
    'data-cube-owner',
    'centered',
  );
  await dialog.getByLabel('Move', { exact: true }).selectOption({ index: 1 });
  await dialog
    .getByLabel('Comment', { exact: true })
    .fill('Double action note');
  await dialog.getByRole('button', { name: 'Save notes', exact: true }).click();
  await page.keyboard.press('Escape');
  await page.getByLabel('Select game', { exact: true }).selectOption('2');
  await expect(page.locator('[data-annotation-kind=game]')).toBeVisible();
  await expect(
    page
      .locator('[data-annotation-kind=game]')
      .getByLabel('Comment', { exact: true }),
  ).toHaveValue('');
});

test('source dialog and main checker notes share one retained draft', async ({
  page,
}) => {
  await seedLocale(page);
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'draft-modal.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Draft ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  const main = page.locator('.analysis-workspace .annotation-panel');
  await expect(main.getByLabel('Comment', { exact: true })).toBeEnabled();
  await main.getByLabel('Comment', { exact: true }).fill('First main draft');
  await page
    .getByRole('button', { name: 'Cube and move notes', exact: true })
    .click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByLabel('Comment', { exact: true })).toHaveValue(
    'First main draft',
  );
  await dialog
    .getByLabel('Comment', { exact: true })
    .fill('Updated modal draft');
  await page.keyboard.press('Escape');
  await expect(main.getByLabel('Comment', { exact: true })).toHaveValue(
    'Updated modal draft',
  );
});

test('delayed cube note search cannot replace a newer game selection', async ({
  page,
}) => {
  await seedLocale(page);
  await page.goto('/');
  const imported = page.waitForResponse(
    (r) => new URL(r.url()).pathname === '/api/imports',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'late-note.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Late ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[double];W[take];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe])`,
    ),
  });
  const { match } = await (await imported).json();
  const catalog = await (
    await page.request.get(`/api/matches/${match.id}/games/1/decisions`)
  ).json();
  const move = catalog.items.find((m: { kind: string }) => m.kind === 'cube');
  const marker = `late-note-${crypto.randomUUID()}`;
  await page.request.put(`/api/moves/${move.id}/annotation`, {
    data: { tags: [], comment: marker, expected_version: 0 },
  });
  await openWorkspace(page, 'library');
  await page
    .locator('.library-search-toolbar')
    .getByRole('button', { name: 'Notes', exact: true })
    .click();
  await page
    .locator('.annotation-search')
    .getByLabel('Text', { exact: true })
    .fill(marker);
  await page.getByRole('button', { name: 'Search notes', exact: true }).click();
  let release = () => {},
    started = () => {};
  const gate = new Promise<void>((resolve) => {
      release = resolve;
    }),
    waiting = new Promise<void>((resolve) => {
      started = resolve;
    });
  await page.route(`**/api/moves/${move.id}`, async (route) => {
    started();
    await gate;
    await route.continue();
  });
  await page
    .locator('.annotation-search-hit')
    .getByRole('button', { name: 'Open position', exact: true })
    .click();
  await waiting;
  await page.getByLabel('Select game', { exact: true }).selectOption('2');
  await expect(page.locator('.board-heading')).toContainText('Game 2');
  release();
  await page.unrouteAll({ behavior: 'wait' });
  await page.waitForTimeout(150);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect(page.locator('.board-heading')).toContainText('Game 2');
});

test('multiple import outcomes and batch tags, move, ZIP export and delete', async ({
  page,
}) => {
  const dbs = [];
  for (const prefix of ['Batch source', 'Batch destination']) {
    const response = await page.request.post('/api/databases', {
      data: { name: `${prefix} ${crypto.randomUUID()}`, description: '' },
    });
    expect(response.status()).toBe(201);
    dbs.push(await response.json());
  }
  await page.addInitScript((id) => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.database', id);
  }, dbs[0].id);
  await page.goto('/');
  const file = (name: string, player: string) => ({
    name,
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[${player}]PB[Opponent]MI[length:5][ws:0][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await page.getByLabel('Match file', { exact: true }).setInputFiles([
    file('first.sgf', 'First player'),
    {
      name: 'invalid.sgf',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from('invalid'),
    },
    file('second.sgf', 'Second player'),
  ]);
  await expect(page.locator('.import-results li')).toHaveCount(3);
  await expect(page.locator('.import-results')).toContainText('Action failed');
  await expect(page.locator('.database-table tbody tr')).toHaveCount(2);
  await page.getByLabel('Select all loaded matches', { exact: true }).check();
  await page
    .locator('.library-batch-bar')
    .getByRole('button', { name: 'Tags', exact: true })
    .click();
  await page
    .getByRole('dialog')
    .getByLabel('Add tags (comma separated)', { exact: true })
    .fill('Training, Cube');
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Tags', exact: true })
    .click();
  await expect(page.locator('.database-table tbody tr').first()).toContainText(
    'training',
  );
  await page.getByLabel('Select all loaded matches', { exact: true }).check();
  await page
    .locator('.library-batch-bar')
    .getByRole('button', { name: 'Move', exact: true })
    .click();
  await page
    .getByRole('dialog')
    .getByLabel('Target database', { exact: true })
    .selectOption(dbs[1].id);
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Move', exact: true })
    .click();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(0);
  await page
    .locator('.database-collections')
    .getByRole('button', { name: new RegExp(dbs[1].name) })
    .click();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(2);
  await page.getByLabel('Select all loaded matches', { exact: true }).check();
  await page
    .locator('.library-batch-bar')
    .getByRole('button', { name: 'Export', exact: true })
    .click();
  const pending = page.waitForEvent('download');
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Export', exact: true })
    .click();
  expect((await pending).suggestedFilename()).toMatch(/\.zip$/);
  await page
    .locator('.library-batch-bar')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Delete', exact: true })
    .click();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(0);
  for (const db of dbs)
    expect(
      (
        await page.request.delete(
          `/api/databases/${db.id}?version=${db.version}&delete_matches=true`,
        )
      ).status(),
    ).toBe(204);
});

test('the library exports the original, SGF, and XG formats', async ({
  page,
}) => {
  const player = `Export ${crypto.randomUUID()}`;
  const source = `(;GM[6]FF[4]PW[${player}]PB[Opponent]MI[length:5][ws:0][bs:0];W[31hefe];B[42qusu])`;
  const uploaded = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: 'typed-export.sgf',
        mimeType: 'application/octet-stream',
        buffer: Buffer.from(source),
      },
    },
  });
  expect(uploaded.ok()).toBe(true);
  const { match } = await uploaded.json();
  await seedLocale(page);
  await gotoWorkspace(page, 'library');
  const row = page
    .locator('.database-table tbody tr')
    .filter({ hasText: player });
  const exportButton = page
    .locator('.library-batch-bar')
    .getByRole('button', { name: 'Export', exact: true });
  const dialog = page.getByRole('dialog');

  for (const [format, filename] of [
    ['original', `${match.id}-typed-export.sgf`],
    ['sgf', `typed-export-${match.id}.sgf`],
    ['xg', `typed-export-${match.id}.xg`],
  ]) {
    await row.getByRole('checkbox').check();
    await exportButton.click();
    await dialog
      .getByLabel('Export format', { exact: true })
      .selectOption(format);
    const pending = page.waitForEvent('download');
    await dialog.getByRole('button', { name: 'Export', exact: true }).click();
    const download = await pending;
    expect(download.suggestedFilename()).toBe(filename);
    const bytes = await readFile(await download.path());
    if (format === 'original') expect(bytes.toString()).toBe(source);
    else if (format === 'sgf') expect(bytes.toString()).toContain('W[31hefe]');
    else expect(bytes.subarray(0, 4).toString()).toBe('RGMH');
    await expect(dialog).toHaveCount(0);
  }
});

test('a native full-match analysis supplies GNUbg PR to the match overview', async ({
  page,
}) => {
  await seedLocale(page);
  await page.goto('/');
  const imported = page.waitForResponse(
    (r) => new URL(r.url()).pathname === '/api/imports',
  );
  const player = `PR ${crypto.randomUUID()}`;
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'native-pr.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[${player}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  const { match } = await (await imported).json();
  const response = await page.request.post(
    `/api/matches/${match.id}/analysis`,
    { data: { plies: 1 } },
  );
  expect(response.status()).toBe(202);
  const job = await response.json();
  await waitForJobCompletion(page, job.id, 20000);
  const completed = await (
    await page.request.get(`/api/jobs/${job.id}`)
  ).json();
  expect(completed.result.performance_rating.method).toBe('gnubg_emg_500_v1');
  await openWorkspace(page, 'library');
  await page.getByLabel('Search matches', { exact: true }).fill(player);
  const row = page.locator('.database-table tbody tr');
  await expect(row).toHaveCount(1);
  await expect(row.locator('td').nth(3)).toHaveText(/^\d+\.\d$/);
  await expect(row.locator('td').nth(5)).toHaveText(/^\d+\.\d$/);
  await expect(row).toContainText('Completed');
});
