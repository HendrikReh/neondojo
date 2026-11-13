import { expect, test } from './test';

test('all-games review follows the listed checker and cube errors across games', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page
    .getByLabel('Match file', { exact: true })
    .setInputFiles('../backend/tests/fixtures/WC2014.xg');
  await page.getByRole('button', { name: 'Analyze game or match …' }).click();
  await page
    .getByLabel('Analysis scope', { exact: true })
    .selectOption('match');
  await page.getByRole('button', { name: 'Analyze game or match …' }).click();
  await page
    .getByLabel('Filter analysis by player', { exact: true })
    .selectOption('0');
  const table = page.locator('.match-moves-table');
  const board = page.getByRole('group', { name: 'Review errors' });
  const heading = page.locator('.board-heading');
  const next = board.getByRole('button', { name: 'Next error', exact: true });
  const previous = board.getByRole('button', {
    name: 'Previous error',
    exact: true,
  });
  await expect(page.getByLabel('Filter by game', { exact: true })).toHaveValue(
    '',
  );
  const first = table.locator('tbody tr').first();
  const second = table.locator('tbody tr').nth(1);
  await expect(first).toContainText('Checker');
  await expect(second).toContainText('Cube');
  await expect(heading).toContainText('Game 1');
  await next.click();
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue(
    '2',
  );
  await expect(board).toContainText(/Error 1 of \d+/);
  await expect(first).toHaveAttribute('aria-current', 'true');
  await next.click();
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue(
    '3',
  );
  await expect(board).toContainText(/Error 2 of \d+/);
  await expect(second).toHaveAttribute('aria-current', 'true');
  await previous.click();
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue(
    '2',
  );
  await expect(board).toContainText(/Error 1 of \d+/);
  await page.getByLabel('Filter by game', { exact: true }).selectOption('3');
  await expect(board).toContainText('Game 3');
  await next.click();
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue(
    '3',
  );
  await expect(board).toContainText('Error 1 of 1');

  await page.getByLabel('Filter by game', { exact: true }).selectOption('');
  await page
    .getByLabel('Filter analysis by player', { exact: true })
    .selectOption('');
  await page.locator('.match-analysis-filters input[type=checkbox]').uncheck();
  await table.locator('tbody tr').nth(49).getByRole('button').click();
  await expect(board).toContainText(/Decision 50 of [\d,]+/);
  await board
    .getByRole('button', { name: 'Next decision', exact: true })
    .click();
  await expect(board).toContainText(/Decision 51 of [\d,]+/);
  await expect(table.locator('tbody tr').first()).toHaveAttribute(
    'aria-current',
    'true',
  );
  await board
    .getByRole('button', { name: 'Previous decision', exact: true })
    .click();
  await expect(board).toContainText(/Decision 50 of [\d,]+/);
  await expect(table.locator('tbody tr').nth(49)).toHaveAttribute(
    'aria-current',
    'true',
  );
});

test('imported decisions work in the error list and next-error navigation without a local job', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  const response = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: `review-${crypto.randomUUID()}.mat`,
        mimeType: 'application/octet-stream',
        buffer: Buffer.from(
          '7 point match\n\n Game 1\n Review : 0                    Opponent : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) 65: 24/18 18/13                31: 8/5 6/5\n 3) 42: 8/4 6/4                    65: 24/18 18/13\n',
        ),
      },
    },
  });
  expect(response.ok(), await response.text()).toBe(true);
  const { match } = await response.json();
  const { items: positions } = await (
    await page.request.get(
      `/api/positions?match_id=${match.id}&game_no=1&limit=100`,
    )
  ).json();
  const { items: moves } = await (
    await page.request.get(`/api/matches/${match.id}/games/1/decisions`)
  ).json();
  const errors = [1, 3].map((index) => ({
    id: `imported:${positions[index].id}`,
    job_id: null,
    position_uuid: positions[index].id,
    move_id: null,
    game_no: 1,
    decision_no: index + 1,
    checker_no: index + 1,
    player: 1,
    kind: 'checker',
    played: positions[index].notation,
    best: positions[index].notation,
    played_equity: -0.442,
    best_equity: -0.207,
    equity_loss: 0.235,
    severity: 'very_bad',
    evaluation_status: 'evaluated',
    dice: positions[index].dice,
    source: 'imported',
    engine: 'eXtreme Gammon',
    depth: 'XGRoller+',
    depth_plies: null,
  }));
  await page.route(`**/api/matches/${match.id}/review-decisions?*`, (route) => {
    const query = new URL(route.request().url()).searchParams;
    const items =
      query.get('player') === '0'
        ? []
        : query.get('kind') === 'cube'
          ? errors.map((row) => ({
              ...row,
              id: `cube:${row.id}`,
              kind: 'cube',
              played: 'No double',
              best: 'Double',
              review_position: {
                ...positions[row.checker_no - 1],
                dice: [0, 0],
              },
              move_id: moves.find(
                (move: { position_uuid: string }) =>
                  move.position_uuid === row.position_uuid,
              ).id,
            }))
          : errors;
    return route.fulfill({ json: { items, next_cursor: null } });
  });
  await page.addInitScript(
    (id) =>
      localStorage.setItem(
        'bgtutor.workspace',
        JSON.stringify({ matchId: id, gameNo: 1 }),
      ),
    match.id,
  );
  await page.goto('/');
  const next = page.getByRole('button', { name: 'Next error', exact: true });
  const counter = page.getByRole('group', { name: 'Review errors' });
  await expect(next).toBeEnabled();
  await expect(page.locator('.match-moves-table tbody tr')).toHaveCount(2);
  await expect(
    page.locator('.match-moves-table tbody tr').first(),
  ).toContainText('eXtreme Gammon');
  await expect(
    page.locator('.match-moves-table .equity-loss').first(),
  ).toHaveText('0.235');
  await next.click();
  await expect(page.locator('.board-heading')).toContainText('Move 2');
  await expect(counter).toContainText('Error 1 of 2');
  await next.click();
  await expect(page.locator('.board-heading')).toContainText('Move 4');
  await expect(counter).toContainText('Error 2 of 2');
  await next.click();
  await expect(page.locator('.board-heading')).toContainText('Move 2');
  await page
    .getByLabel('Filter analysis by decision', { exact: true })
    .selectOption('cube');
  const board = page.locator('.board-panel');
  await expect(
    board.getByRole('group', { name: 'Review errors' }),
  ).toContainText('Game 1 · Both players · Cube');
  await expect(board.getByRole('button', { name: 'Next error' })).toBeVisible();
  await next.click();
  await expect(board.locator('.board-heading')).toContainText('Move 4');
  await expect(board.locator('[data-dice]')).toHaveAttribute(
    'data-dice',
    '0-0',
  );
  await expect(board.getByText('No double', { exact: true })).toBeVisible();
  await next.click();
  await expect(board.locator('.board-heading')).toContainText('Move 2');
  await board.getByRole('button', { name: 'Previous error' }).click();
  await expect(board.locator('.board-heading')).toContainText('Move 4');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page
    .locator('.match-analysis-filters')
    .getByRole('combobox')
    .first()
    .selectOption('0');
  await expect(page.locator('.match-moves-table tbody tr')).toHaveCount(0);
  await expect(next).toBeDisabled();
  await expect(counter).toContainText('No decisions match these filters');
  await counter
    .getByRole('button', { name: 'Reset selection filters' })
    .click();
  await expect(page.locator('.match-moves-table tbody tr')).toHaveCount(2);
  await expect(page.locator('.board-heading')).toContainText('Move 4');
});

test('real XG review opens checker and cube decisions with original source and compact desktop comparisons', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'de'));
  const { readFileSync } = await import('node:fs');
  const response = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: 'review-WC2014.xg',
        mimeType: 'application/octet-stream',
        buffer: readFileSync('../backend/tests/fixtures/WC2014.xg'),
      },
    },
  });
  expect(response.ok(), await response.text()).toBe(true);
  const { match } = await response.json();
  await page.addInitScript(
    (id) =>
      localStorage.setItem(
        'bgtutor.workspace',
        JSON.stringify({ matchId: id, gameNo: 1 }),
      ),
    match.id,
  );
  const reviews = await page.request.get(
    `/api/matches/${match.id}/review-decisions?game_no=1&errors_only=false&limit=100`,
  );
  expect(reviews.ok(), await reviews.text()).toBe(true);
  const { items } = await reviews.json();
  const checker = items.find((row: { kind: string }) => row.kind === 'checker');
  expect(checker).toBeTruthy();
  await page.goto('/');
  await page.locator('.match-analysis-filters input[type=checkbox]').uncheck();
  const table = page.locator('.match-moves-table');
  await expect(table.locator('tbody tr').first()).toBeVisible();
  const checkerRow = table
    .locator('tbody tr')
    .filter({ hasText: checker.played })
    .filter({ hasText: 'eXtreme Gammon' })
    .first();
  await checkerRow.getByRole('button').click();
  await expect(page.locator('.board-heading')).toContainText(
    `Zug ${checker.checker_no}`,
  );
  await expect(
    page.getByRole('tab', { name: 'Importierte Analyse', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(
    page.locator('.candidate-comparison-table .candidate-marker-played'),
  ).toBeVisible();
  for (const name of ['Gesamt', 'Einfach', 'Gammon', 'Backgammon']) {
    await expect(
      page.getByRole('columnheader', { name, exact: true }),
    ).toBeVisible();
  }
  await expect(page.locator('.candidate-comparison-table')).toContainText(
    'Verlust',
  );
  await expect(
    page.locator('.board-shell a[href*="bgblitz.com/themes.html"]'),
  ).toHaveCount(0);
  const comparison = page.locator('.candidate-comparison-table');
  let overviewTableWidth: number | undefined;
  let previousBoardWidth: number | undefined;
  const boardWidth = () =>
    page
      .locator('.board-panel .board-shell')
      .evaluate((board) => board.getBoundingClientRect().width);
  for (const viewport of [
    { width: 1800, height: 1169 },
    { width: 2560, height: 1440 },
    { width: 3840, height: 2160 },
  ]) {
    await page.setViewportSize(viewport);
    if (previousBoardWidth !== undefined) {
      await expect.poll(boardWidth).toBeGreaterThan(previousBoardWidth + 50);
      expect(await boardWidth()).toBeLessThan(previousBoardWidth + 140);
    }
    previousBoardWidth = await boardWidth();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: `/tmp/bgtutor-review-${viewport.width}.png`,
    });
    const candidateWidth = await page
      .locator('.source-candidates')
      .evaluate((wrapper) => ({
        available: wrapper.clientWidth,
        used: wrapper.scrollWidth,
      }));
    expect(
      candidateWidth.used,
      `candidate table at ${viewport.width}px`,
    ).toBeLessThanOrEqual(candidateWidth.available);
    const width = (await comparison.boundingBox())!.width;
    overviewTableWidth ??= width;
    expect(width, `candidate table at ${viewport.width}px`).toBeCloseTo(
      overviewTableWidth,
      0,
    );
  }
  const cubeReviews = await page.request.get(
    `/api/matches/${match.id}/review-decisions?game_no=1&kind=cube&errors_only=false&limit=100`,
  );
  const { items: cubes } = await cubeReviews.json();
  expect(cubes.length).toBeGreaterThan(0);
  await page.locator('.match-analysis-filters input[type=checkbox]').uncheck();
  await page
    .getByLabel('Analyse nach Entscheidung filtern', { exact: true })
    .selectOption('cube');
  await expect(table.locator('tbody tr').first()).toBeVisible();
  await table.locator('tbody tr').first().getByRole('button').click();
  await expect(page.locator('.board-panel [data-dice]')).toHaveAttribute(
    'data-dice',
    '0-0',
  );
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.getByRole('button', { name: 'Cube-Details', exact: true }).click();
  await expect(
    page.getByRole('dialog').locator('.imported-move-analysis'),
  ).toContainText('eXtreme Gammon');
  await expect(
    page.getByRole('dialog').locator('.source-record select'),
  ).toHaveCount(0);
  await expect(
    page
      .getByRole('dialog')
      .locator('.imported-move-analysis table.candidates'),
  ).toBeVisible();
  expect(cubes[0].review_position.dice).toEqual([0, 0]);
  await expect(page.getByRole('dialog').locator('[data-dice]')).toHaveAttribute(
    'data-dice',
    '0-0',
  );
  await expect(
    page.getByRole('dialog').locator('[data-die-owner]'),
  ).toHaveCount(0);
});
