import { openMatchCalculation, openPositionCalculation } from './analysis-controls';
import { expect, test } from './test';
import { readFileSync } from 'node:fs';
test.use({ actionTimeout: 10000 });

test('compares saved catalog analyses without filtering them by calculation engine', async ({
  page,
}, info) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  const summary = (engine: string, plies: number, loss: number) => ({
    engine,
    plies,
    recorded_depth: `${plies}-ply`,
    total_games: 1,
    completed_games: 1,
    total_moves: 2,
    analyzed_moves: 2,
    performance_rating: null,
    players: [0, 1].map((player) => ({
      player,
      checker_decisions: 1,
      cube_decisions: 1,
      checker_equity_loss: player === 0 ? loss : 0,
      cube_equity_loss: 0,
      unavailable: 0,
    })),
  });
  const saved = [
    {
      id: 'gnu-four',
      engine: 'GNU Backgammon 1.08.003',
      plies: 4,
      matchLoss: 0.111,
      gameLoss: 0.444,
    },
    {
      id: 'sage-four',
      engine: 'Open Sage 2.0.20260907',
      plies: 4,
      matchLoss: 0.999,
      gameLoss: 0.555,
    },
    {
      id: 'gnu-three',
      engine: 'GNU Backgammon 1.08.003',
      plies: 3,
      matchLoss: 0.222,
      gameLoss: 0.222,
    },
  ];
  const entries = saved.map((run) => ({
    id: `job:${run.id}`,
    source: 'native',
    scope: 'match',
    engine: run.engine,
    depth: `${run.plies}-ply cubeful`,
    game_no: null,
    position_id: null,
    created_at: '2026-11-12T12:00:00Z',
    status: 'completed',
    complete: true,
    checker_count: 2,
    cube_count: 2,
    job: null,
  }));
  await page.route(/\/api\/matches\/[^/]+\/analyses(?:\?.*)?$/, (route) =>
    route.fulfill({
      json: {
        match_id: new URL(route.request().url()).pathname.split('/')[3],
        items: entries,
        selected: entries[0],
        selected_id: null,
        next_cursor: null,
      },
    }),
  );
  await page.route(
    /\/api\/matches\/[^/]+\/analysis-statistics(?:\?.*)?$/,
    (route) => {
      const id = new URL(route.request().url()).searchParams.get('analysis_id');
      const run = saved.find((item) => `job:${item.id}` === id)!;
      return route.fulfill({
        json: {
          analysis_id: id,
          error_breakdown: null,
          summary: summary(run.engine, run.plies, run.matchLoss),
          games: [
            {
              game_no: 1,
              white_score: 0,
              black_score: 0,
              crawford: false,
              analyzed: true,
              summary: summary(run.engine, run.plies, run.gameLoss),
              error_breakdown: null,
            },
          ],
        },
      });
    },
  );
  await page.route('**/api/positions/*/analysis-result?*', (route) =>
    route.fulfill({ json: null }),
  );
  await page.route('**/api/matches/*/review-decisions?*', (route) =>
    route.fulfill({ json: { items: [], next_cursor: null } }),
  );
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'comparison.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Comparison ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await page.getByText('Compare saved analyses', { exact: true }).click();
  const first = page.getByRole('combobox', { name: 'Comparison analysis 1' });
  const second = page.getByRole('combobox', { name: 'Comparison analysis 2' });
  const table = page.locator('.statistics-comparison-table');
  const loss = table.getByRole('row', { name: /^Checker equity loss/ }).first();
  await expect(first).toHaveValue('job:gnu-four');
  await expect(second).toHaveValue('job:sage-four');
  await expect(loss.getByRole('cell')).toHaveText(['0.111', '0.999']);
  await expect(first.locator('option')).toHaveCount(3);
  await expect(second.locator('option')).toHaveCount(3);
  await expect(loss.getByRole('cell')).toHaveText(['0.111', '0.999']);
  await first.selectOption('job:gnu-three');
  await expect(loss.getByRole('cell')).toHaveText(['0.222', '0.999']);
  // Both independent columns may compare saved results from the same engine.
  await second.selectOption('job:gnu-four');
  await expect(loss.getByRole('cell')).toHaveText(['0.222', '0.111']);
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(first).toBeVisible();
    await expect(second).toBeVisible();
    expect(
      await table.evaluate(
        (element) =>
          element.parentElement!.scrollWidth <=
          element.parentElement!.clientWidth,
      ),
    ).toBe(true);
    await table.screenshot({
      path: info.outputPath(`match-analysis-${width}.png`),
    });
  }
  await expect(first).toHaveValue('job:gnu-three');
  await expect(second).toHaveValue('job:gnu-four');
  await expect(loss.getByRole('cell')).toHaveText(['0.222', '0.111']);
});

function analyzedGameFile() {
  return {
    name: 'preview-sources.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      readFileSync(
        '../backend/tests/fixtures/analysis-game1.sgf',
        'utf8',
      ).replace(/PW\[[^\]]*\]/, `PW[Preview sources ${crypto.randomUUID()}]`),
    ),
  };
}

test('played move arrows and before/after remain tied to the original decision', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'preview.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Preview ${crypto.randomUUID()}]PB[Opponent]MI[length:7];W[31hefe];B[42qusu])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  const code = await page.locator('.position-code code').innerText();
  const arrows = page.getByRole('button', { name: 'Arrows', exact: true });
  await expect(arrows).toHaveAttribute('aria-pressed', 'true');
  await expect(page.locator('.analysis-workspace .move-arrows')).toBeVisible();
  await page.getByRole('button', { name: 'Initial', exact: true }).click();
  await expect(page.locator('.analysis-workspace .move-arrows')).toHaveCount(0);
  await expect(page.locator('.board-shell .player-bar').last()).toContainText(
    '167 pips',
  );
  await page.getByRole('button', { name: 'Final', exact: true }).click();
  await expect(page.locator('.board-shell .player-bar').last()).toContainText(
    '163 pips',
  );
  await expect(page.locator('.position-code code')).toHaveText(code);
  await expect(page.locator('.board-review-help')).toContainText(
    'before this move',
  );
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.board-review-selection')).toHaveCount(0);
  await expect(page.locator('.board-navigation')).toContainText('8/4 6/4');
  await expect(page.locator('.board-shell .player-bar').first()).toContainText(
    '161 pips',
  );
});

test('file, book and native candidates preview explicitly and source changes restore the played move', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page
    .getByLabel('Match file', { exact: true })
    .setInputFiles(analyzedGameFile());
  await expect(
    page.getByLabel('Displayed analysis', { exact: true }).locator('option[value^="imported:"]'),
  ).toHaveCount(1);
  const imported = page.locator('.analysis-panel > .source-analysis');
  await imported.getByRole('button', { name: /^Preview move 2:/ }).click();
  await expect(page.locator('.board-review-selection')).toContainText(
    'Candidate 2',
  );
  await expect(page.locator('.board-review-selection')).toContainText(
    'GNU Backgammon',
  );
  await expect(page.locator('.board-shell .move-arrows')).toBeVisible();
  await page.getByRole('button', { name: 'Final', exact: true }).click();
  await expect(page.locator('.board-shell')).toHaveAttribute(
    'data-preview-mode',
    'after',
  );
  await page.locator('.analysis-book-reference > summary').click();
  await expect(page.locator('.board-review-selection')).toHaveCount(0);
  await page
    .locator('.analysis-book-reference')
    .getByRole('button', { name: /^Preview move 1:/ })
    .click();
  await expect(page.locator('.board-review-selection')).toContainText(
    'GNU Backgammon',
  );
  await expect(
    page.locator('.board-review-selection .engine-label-details'),
  ).toHaveAttribute('aria-description', /BackgammonDB/);
  await page
    .getByRole('button', { name: 'Reset preview', exact: true })
    .click();
  await expect(page.locator('.board-review-selection')).toHaveCount(0);
  await (
    await openPositionCalculation(page)
  )
    .getByRole('button', { name: 'Analyze position', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Show calculated result', exact: true })
    .click();
  const native = page.locator('.analysis-panel > .candidate-comparison');
  await native.getByRole('button', { name: /^Preview move 1:/ }).click();
  await expect(page.locator('.board-review-selection')).toContainText(
    'Candidate 1',
  );
  await expect(page.locator('.board-review-selection')).toContainText(
    'GNU Backgammon',
  );
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.board-review-selection')).toHaveCount(0);
});

for (const [layout, viewport] of [
  ['mobile', { width: 390, height: 900 }],
  ['desktop', { width: 1440, height: 1100 }],
] as const) {
  test(`returning to the played move reveals its marked row in the scrolled history on ${layout}`, async ({
    page,
  }) => {
    await page.setViewportSize(viewport);
    await page.addInitScript(() =>
      localStorage.setItem('bgtutor.locale', 'de'),
    );
    await page.goto('/');
    await page
      .getByLabel('Partiedatei', { exact: true })
      .setInputFiles('../backend/tests/fixtures/WC2014.xg');
    await page.getByLabel('Spiel auswählen', { exact: true }).selectOption('6');
    await expect(page.locator('.board-heading')).toContainText('Spiel 6');
    await expect(
      page.getByLabel('Angezeigte Analyse', { exact: true }).locator('option[value^="imported:"]'),
    ).toHaveCount(1);
    await page.getByRole('button', { name: /^Zug 2 ansehen:/ }).click();

    const moveList = page.locator('.analysis-workspace .moves-list');
    const playedMove = moveList.getByRole('button').first();
    await moveList.scrollIntoViewIfNeeded();
    await expect(playedMove).toHaveAttribute('aria-current', 'step');
    await expect(playedMove).toBeInViewport({ ratio: 0.99 });
    await moveList.evaluate((list) => {
      list.scrollTop = list.scrollHeight;
    });
    await expect(playedMove).not.toBeInViewport();

    await page
      .getByRole('button', { name: 'Zum gespielten Zug', exact: true })
      .click();
    await expect(playedMove).toBeFocused();
    await expect(playedMove).toHaveAttribute('aria-current', 'step');
    await expect(playedMove).toHaveCSS('outline-style', 'solid');
    await expect(playedMove).toBeInViewport({ ratio: 0.99 });
    await expect(page.locator('.board-review-selection')).toHaveCount(0);
  });
}

test('native match and game metrics open the exact visible decision filters', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'statistics-drilldown.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Stats ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  await openMatchCalculation(page);
  await page
    .getByLabel('Analysis scope', { exact: true })
    .selectOption('match');
  await page
    .getByRole('button', { name: 'Analyze entire match', exact: true })
    .click();
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  const stats = page.locator('.match-statistics');
  await expect(stats.locator('.statistics-games-table tbody tr')).toHaveCount(
    2,
  );
  await expect(stats.locator('.statistics-coverage').first()).toContainText(
    '2 of 2',
  );
  await expect(stats.locator('.statistics-source')).toContainText(
    'GNU Backgammon',
  );
  const request = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname.endsWith('/review-decisions') &&
      new URL(r.url()).searchParams.get('kind') === 'cube' &&
      new URL(r.url()).searchParams.get('player') === '0',
  );
  await stats
    .locator('.statistics-summary [data-statistics-metric="cube"]')
    .first()
    .click();
  expect((await request).status()).toBe(200);
  await expect(
    page.getByRole('tab', { name: 'Errors', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(
    page.getByLabel('Filter analysis by player', { exact: true }),
  ).toHaveValue('0');
  await expect(
    page.getByLabel('Filter analysis by decision', { exact: true }),
  ).toHaveValue('cube');
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  const game2 = stats.locator('.statistics-games-table tbody tr').nth(1);
  await game2.getByRole('button', { name: 'Open game 2', exact: true }).click();
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue(
    '2',
  );
  await game2.locator('[data-statistics-metric="checker"]').first().click();
  await expect(page.getByLabel('Filter by game', { exact: true })).toHaveValue(
    '2',
  );
  await expect(
    page.getByLabel('Filter analysis by decision', { exact: true }),
  ).toHaveValue('checker');
  await expect(page.locator('.match-moves-table tbody tr')).toHaveCount(1);
  // The whole-match decision filter stays on game 2 while browsing another game.
  await page.getByLabel('Select game', { exact: true }).selectOption('1');
  await expect(page.locator('.board-heading')).toContainText('Game 1');
  await expect(page.getByLabel('Filter by game', { exact: true })).toHaveValue('2');
  const nextError = page.waitForRequest(
    (r) =>
      new URL(r.url()).pathname.endsWith('/review-decisions') &&
      new URL(r.url()).searchParams.get('limit') === '100',
  );
  await page.getByRole('button', { name: 'Next decision', exact: true }).click();
  const nextQuery = new URL((await nextError).url()).searchParams;
  expect(nextQuery.get('game_no')).toBe('2');
  expect(nextQuery.get('errors_only')).toBe('false');
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue('2');
  await expect(
    page.getByRole('group', { name: 'Review errors' }),
  ).toContainText('Decision 1 of 1');
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await stats
    .locator('.statistics-summary [data-statistics-metric="very_bad"]')
    .first()
    .click();
  await expect(
    page.getByLabel('Filter by severity', { exact: true }),
  ).toHaveValue('very_bad');
  await expect(
    page.getByLabel('Filter analysis by decision', { exact: true }),
  ).toHaveValue('');
  await expect(page.getByLabel('Filter by game', { exact: true })).toHaveValue(
    '',
  );
  await page
    .getByRole('button', { name: 'Reset filters', exact: true })
    .click();
  await expect(
    page.getByLabel('Filter by severity', { exact: true }),
  ).toHaveValue('');
  await expect(
    page.getByLabel('Filter analysis by player', { exact: true }),
  ).toHaveValue('');
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await expect(stats.locator('.statistics-source')).toContainText(
    'GNU Backgammon',
  );
  await expect(
    page.getByLabel('Displayed analysis', { exact: true }),
  ).toContainText('Entire match');
});

test('a delayed next-error response cannot undo manual move navigation', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'next-error-race.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Error race ${crypto.randomUUID()}]PB[Opponent]MI[length:7];W[31xufe];B[42aesu])`,
    ),
  });
  await page.getByRole('tab', { name: 'Errors', exact: true }).click();
  await openMatchCalculation(page);
  await page.getByRole('button', { name: 'Analyze game', exact: true }).click();
  await expect(
    page.getByText('Game analysis completed', { exact: true }),
  ).toBeVisible();
  let release = () => {},
    started = () => {},
    finished = () => {};
  const gate = new Promise<void>((resolve) => {
      release = resolve;
    }),
    waiting = new Promise<void>((resolve) => {
      started = resolve;
    }),
    fulfilled = new Promise<void>((resolve) => {
      finished = resolve;
    });
  await expect(page.getByRole('button', { name: /^Next error/ })).toBeEnabled();
  await page.route(
    '**/api/matches/*/review-decisions?**',
    async (route) => {
      const response = await route.fetch();
      started();
      await gate;
      await route.fulfill({ response }).catch(() => {});
      finished();
    },
    { times: 1 },
  );
  try {
    await page.getByRole('button', { name: /^Next error/ }).click();
    await waiting;
    await page.getByRole('button', { name: 'Next move', exact: true }).click();
    await expect(page.locator('.board-heading')).toContainText('Move 2');
  } finally {
    release();
  }
  await fulfilled;
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  await expect(page.locator('.board-heading')).toContainText('Move 2');
});

test('all board themes render the same arrows and projected checker counts', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'preview-themes.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Theme preview ${crypto.randomUUID()}]PB[Opponent]MI[length:7];W[31hefe];B[42qusu])`,
    ),
  });
  await expect(page.locator('.analysis-workspace .board-shell')).toBeVisible();
  const themes = [
    'forest',
    'walnut',
    'slate',
    ...JSON.parse(readFileSync('src/generated-themes.json', 'utf8')).map(
      (theme: { id: string }) => theme.id,
    ),
  ];
  for (const theme of themes) {
    await expect
      .poll(() =>
        page.evaluate(() =>
          localStorage.getItem('bgtutor.configuration-pending.v1'),
        ),
      )
      .toBeNull();
    await page.evaluate(async (theme) => {
      const state = await (await fetch('/api/configuration')).json();
      const configuration = {
        ...state.current_configuration,
        appearance: {
          ...state.current_configuration.appearance,
          board_theme: theme,
          board_view: 'arrows',
        },
      };
      const response = await fetch('/api/configuration', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          expected_revision: state.revision,
          configuration,
        }),
      });
      if (!response.ok) throw new Error(await response.text());
    }, theme);
    await page.reload();
    const board = page.locator('.analysis-workspace .board-shell');
    await expect(board).toHaveAttribute('data-preview-mode', 'arrows');
    await expect(board.locator('.move-arrows > g')).toHaveCount(2);
    const steps = await board
      .locator('.move-arrows > g')
      .evaluateAll((elements) =>
        elements
          .map(
            (e) =>
              `${e.getAttribute('data-from')}:${e.getAttribute('data-to')}:${e.getAttribute('data-die')}`,
          )
          .sort(),
      );
    expect(steps, theme).toEqual(['6:5:1', '8:5:3']);
    await page.getByRole('button', { name: 'Final', exact: true }).click();
    await expect(board.locator('.player-bar').last()).toContainText('163 pips');
    await expect(board.locator('.move-arrows')).toHaveCount(0);
  }
});
