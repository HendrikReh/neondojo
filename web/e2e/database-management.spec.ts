import { expect, test } from './test';

test('collections, independent copies, filters and metadata work together', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page.getByRole('button', { name: 'New database', exact: true }).click();
  const name = `Library ${crypto.randomUUID()}`;
  await page.getByRole('dialog').getByLabel('Name', { exact: true }).fill(name);
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Save', exact: true })
    .click();
  await expect(page.locator('.database-sidebar')).toContainText(name);
  const whitePlayer = `Library ${crypto.randomUUID()}`;
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'library.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[${whitePlayer}]PB[Opponent]DT[2026-11-08]EV[Test Open]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
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
    .getByRole('button', {
      name: 'Edit match details: library.sgf',
      exact: true,
    })
    .click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Name', { exact: true }).fill('Final training match');
  await dialog.getByLabel('Tournament', { exact: true }).fill('Autumn Open');
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
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page
    .locator('.database-table tbody tr')
    .first()
    .locator('td')
    .nth(3)
    .dblclick();
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
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
    .getByLabel('Tournament', { exact: true })
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
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await expect(page.locator('.analysis-welcome')).toBeVisible();
});

test('a game and an explicit cube action have separate searchable notes', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
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
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
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
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
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
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page.locator('.annotation-search summary').click();
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
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.database', id);
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

test('a native full-match analysis supplies GNUbg PR to the match overview', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
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
    { data: { plies: 0 } },
  );
  expect(response.status()).toBe(202);
  const job = await response.json();
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
      { timeout: 20000 },
    )
    .toBe('completed');
  const completed = await (
    await page.request.get(`/api/jobs/${job.id}`)
  ).json();
  expect(completed.result.performance_rating.method).toBe('gnubg_emg_500_v1');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page.getByLabel('Search matches', { exact: true }).fill(player);
  const row = page.locator('.database-table tbody tr');
  await expect(row).toHaveCount(1);
  await expect(row.locator('td').nth(3)).toHaveText(/\d+\.\d{2}/);
  await expect(row.locator('td').nth(5)).toHaveText(/\d+\.\d{2}/);
  await expect(row).toContainText('Completed');
});
