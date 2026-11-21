import { expect, test } from './test';

test('all-games Cube review wraps across games', async ({ page }) => {
  test.setTimeout(90_000);
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  await page
    .getByLabel('Match file', { exact: true })
    .setInputFiles('../backend/tests/fixtures/WC2014.xg');
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  await page
    .getByLabel('Filter analysis by player', { exact: true })
    .selectOption('0');
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
  await expect(heading).toContainText('Game 1');
  await expect(board).toContainText(
    'All games in this analysis · Akiko Yazawa',
  );
  await expect(board.locator('.board-error-count')).toHaveText(
    /^Errors: [1-9]/,
  );
  await next.click();
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue(
    '3',
  );
  await expect(board).toContainText(/Error 1 of \d+/);
  const responseOptions = page.getByRole('region', { name: 'Cube evaluation' });
  await expect(responseOptions).toContainText(
    'Values from Akiko Yazawa’s perspective',
  );
  await expect(
    responseOptions.getByRole('row', { name: /Take.*-0\.942/ }),
  ).toBeVisible();
  await expect(
    responseOptions.getByRole('row', { name: /Pass.*-1\.000/ }),
  ).toBeVisible();
  await previous.click();
  await expect(page.getByLabel('Select game', { exact: true })).not.toHaveValue(
    '3',
  );
  await next.click();
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue(
    '3',
  );
  await page.getByLabel('Filter by game', { exact: true }).selectOption('3');
  await expect(board).toContainText('Game 3');
  await next.click();
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue(
    '3',
  );
  await expect(board).toContainText('Error 1 of 1');
});

test('imported decisions work in error navigation without a local job', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
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
  const importedEntry = {
    id: `imported:${match.id}`,
    source: 'imported',
    scope: 'match',
    engine: 'eXtreme Gammon',
    depth: 'XGRoller+',
    game_no: null,
    position_id: null,
    created_at: match.created_at,
    status: 'completed',
    complete: false,
    checker_count: errors.length,
    cube_count: errors.length,
    job: null,
  };
  await page.route(`**/api/matches/${match.id}/analyses?*`, (route) =>
    route.fulfill({
      json: {
        match_id: match.id,
        items: [importedEntry],
        selected: importedEntry,
        selected_id: null,
        next_cursor: null,
      },
    }),
  );
  await page.route('**/api/positions/*/analysis-result?*', (route) =>
    route.fulfill({ json: null }),
  );
  await page.route('**/api/positions/*/jobs?*', (route) =>
    route.fulfill({ json: [] }),
  );
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
    expect(query.get('analysis_id')).toBe(importedEntry.id);
    return route.fulfill({
      json: { items, next_cursor: null, total: items.length },
    });
  });
  await page.addInitScript(
    (id) =>
      localStorage.setItem(
        'neondojo.workspace',
        JSON.stringify({ matchId: id, gameNo: 1 }),
      ),
    match.id,
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  const next = page.getByRole('button', { name: 'Next error', exact: true });
  const counter = page.getByRole('group', { name: 'Review errors' });
  await expect(next).toBeEnabled();
  await expect(counter).toContainText('Errors: 2');
  await next.click();
  await expect(page.locator('.board-heading')).toContainText('Move 2');
  await expect(counter).toContainText('Error 1 of 2');
  const comparison = page.locator('.board-panel .board-decision-comparison');
  await expect(comparison).toContainText('eXtreme Gammon');
  await expect(comparison).toContainText('Loss 0.235');
  const board = page.locator('.board-panel');
  await expect(
    board.getByRole('group', { name: 'Review errors' }),
  ).toContainText('All games in this analysis · Both players · Cube');
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
  await expect(next).toBeDisabled();
  await expect(counter).toContainText('No decisions match these filters');
  await counter
    .getByRole('button', { name: 'Reset selection filters' })
    .click();
  await expect(next).toBeEnabled();
  await expect(counter).toContainText(/Errors: 2|Error [12] of 2/);
  await expect(page.locator('.board-heading')).toContainText('Move 4');
});

test('real XG review opens checker and cube decisions with original source and responsive desktop comparisons', async ({
  page,
}, testInfo) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'de'));
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
        'neondojo.workspace',
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
  await page
    .locator(
      `.analysis-workspace .move-history-row[data-position-id="${checker.position_uuid}"]`,
    )
    .click();
  await expect(page.locator('.board-heading')).toContainText(
    `Zug ${checker.checker_no}`,
  );
  await expect(
    page.getByLabel('Angezeigte Analyse', { exact: true }),
  ).toHaveAttribute('title', /eXtreme Gammon · Importiert/);
  await expect(
    page.locator(
      '.analysis-panel > .source-analysis .candidate-comparison-table .candidate-marker-played',
    ),
  ).toBeVisible();
  for (const side of ['Gewinne', 'Niederlagen']) {
    for (const outcome of ['Gesamt', 'Gammon', 'Backgammon']) {
      await expect(
        page.getByRole('columnheader', {
          name: `${side}: ${outcome}`,
          exact: true,
        }),
      ).toBeVisible();
    }
  }
  await expect(
    page.locator(
      '.analysis-panel > .source-analysis .candidate-comparison-table',
    ),
  ).toContainText('Verlust');
  await expect(
    page.locator('.board-shell a[href*="bgblitz.com/themes.html"]'),
  ).toHaveCount(0);
  const comparison = page.locator(
    '.analysis-panel > .source-analysis .candidate-comparison-table',
  );
  let previousTableWidth: number | undefined;
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
    }
    previousBoardWidth = await boardWidth();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: test.info().outputPath(`review-${viewport.width}.png`),
    });
    const candidateWidth = await page
      .locator('.analysis-panel > .source-analysis .source-candidates')
      .evaluate((wrapper) => ({
        available: wrapper.clientWidth,
        used: wrapper.scrollWidth,
      }));
    expect(
      candidateWidth.used,
      `candidate table at ${viewport.width}px`,
    ).toBeLessThanOrEqual(candidateWidth.available);
    const width = (await comparison.boundingBox())!.width;
    if (previousTableWidth !== undefined)
      expect(width).toBeGreaterThanOrEqual(previousTableWidth - 1);
    previousTableWidth = width;
  }
  const cubeReviews = await page.request.get(
    `/api/matches/${match.id}/review-decisions?game_no=1&kind=cube&errors_only=false&limit=100`,
  );
  const { items: cubes } = await cubeReviews.json();
  expect(cubes.length).toBeGreaterThan(0);
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  const nextDecision = page.getByRole('button', {
    name: 'Nächste Entscheidung',
    exact: true,
  });
  await page.locator('.match-analysis-filters input[type=checkbox]').uncheck();
  await expect(nextDecision).toBeEnabled();
  await nextDecision.click();
  await expect(page.locator('.board-panel [data-dice]')).toHaveAttribute(
    'data-dice',
    '0-0',
  );
  await expect(page.getByRole('dialog')).toHaveCount(0);
  const options = page.getByRole('region', { name: 'Cube-Bewertung' });
  await expect(
    options.getByRole('row', { name: /Nicht doppeln/ }),
  ).toBeVisible();
  await expect(
    options.getByRole('row', { name: /Doppeln \/ annehmen/ }),
  ).toBeVisible();
  await expect(
    options.getByRole('row', { name: /Doppeln \/ passen/ }),
  ).toBeVisible();
  await expect(options).toContainText(
    `Werte aus Sicht von ${cubes[0].player === 0 ? match.white : match.black}`,
  );
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(options).toBeInViewport();
    const pane = (await options.boundingBox())!;
    const board = (await page.locator('.board-panel').boundingBox())!;
    expect(pane.x).toBeGreaterThanOrEqual(board.x + board.width);
    await page.screenshot({
      path: testInfo.outputPath(`cube-evaluation-${width}.png`),
    });
  }
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

test('the board crosses a late review page without scanning the earlier pages again', async ({
  page,
}) => {
  test.setTimeout(90_000);
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  const source = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: 'page-boundary.xg',
        mimeType: 'application/octet-stream',
        buffer: (await import('node:fs')).readFileSync(
          '../backend/tests/fixtures/WC2014.xg',
        ),
      },
    },
  });
  expect(source.ok(), await source.text()).toBe(true);
  const { match } = await source.json();
  const pageSize = 50;
  const startPage = 3;
  const reviews = await page.request.get(
    `/api/matches/${match.id}/review-decisions?kind=cube&errors_only=false&page=${startPage}&limit=${pageSize}`,
  );
  expect(reviews.ok(), await reviews.text()).toBe(true);
  const { items } =
    (await reviews.json()) as import('../src/review-decisions-api').ReviewDecisionPage;
  expect(items).toHaveLength(pageSize);
  const anchorIndex = pageSize - 3;
  const anchor = items[anchorIndex];
  const game = await (
    await page.request.get(
      `/api/positions?match_id=${match.id}&game_no=${anchor.game_no}&limit=100`,
    )
  ).json();
  const ply = anchor.next_checker_no ?? anchor.checker_no;
  expect(ply).toBeTruthy();
  const position = game.items.find((move: { ply: number }) => move.ply === ply);
  expect(position).toBeTruthy();
  await page.addInitScript(
    ({ matchId, gameNo }) => {
      localStorage.setItem(
        'neondojo.workspace',
        JSON.stringify({ matchId, gameNo }),
      );
    },
    { matchId: match.id, gameNo: anchor.game_no },
  );
  await page.goto('/');
  await page
    .locator(
      `.analysis-workspace .move-history-row[data-position-id="${position.id}"]`,
    )
    .click();
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  await page.locator('.match-analysis-filters input[type=checkbox]').uncheck();
  const board = page.getByRole('group', { name: 'Review errors' });
  const next = board.getByRole('button', {
    name: 'Next decision',
    exact: true,
  });
  const previous = board.getByRole('button', {
    name: 'Previous decision',
    exact: true,
  });
  const ordinal = startPage * pageSize;
  await next.click();
  const count = board.locator('.board-error-count');
  await expect(count).toHaveText(/^Decision \d+ of /);
  const current = Number(
    (await count.innerText()).match(/^Decision (\d+) of /)?.[1],
  );
  expect(current).toBeGreaterThanOrEqual(ordinal - 3);
  expect(current).toBeLessThanOrEqual(ordinal);
  for (let step = current + 1; step <= ordinal; step++) {
    await next.click();
    await expect(count).toContainText(`Decision ${step} of`);
  }

  const queries: URLSearchParams[] = [];
  const collect = (request: import('@playwright/test').Request) => {
    const url = new URL(request.url());
    if (url.pathname.endsWith('/review-decisions'))
      queries.push(url.searchParams);
  };
  page.on('request', collect);
  await next.click();
  await expect(board).toContainText(`Decision ${ordinal + 1} of`);
  await previous.click();
  await expect(board).toContainText(`Decision ${ordinal} of`);
  page.off('request', collect);
  expect(queries.map((query) => query.get('page'))).toContain(
    String(startPage + 1),
  );
  expect(
    queries.every((query) =>
      [String(startPage), String(startPage + 1)].includes(
        query.get('page') ?? '',
      ),
    ),
  ).toBe(true);
});
