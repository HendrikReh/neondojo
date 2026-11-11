import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';
test.use({ actionTimeout: 10000 });

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
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'preview-sources.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      readFileSync(
        '../backend/tests/fixtures/analysis-game1.sgf',
        'utf8',
      ).replace(/PW\[[^\]]*\]/, `PW[Preview sources ${crypto.randomUUID()}]`),
    ),
  });
  await page
    .getByRole('tab', { name: 'Imported analysis', exact: true })
    .click();
  const imported = page.locator('.source-analysis');
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
  await page.getByRole('tab', { name: 'Opening book', exact: true }).click();
  await expect(page.locator('.board-review-selection')).toHaveCount(0);
  await imported
    .getByRole('button', { name: 'Preview best move', exact: true })
    .click();
  await expect(page.locator('.board-review-selection')).toContainText(
    'BackgammonDB',
  );
  await page
    .getByRole('button', { name: 'Reset preview', exact: true })
    .click();
  await expect(page.locator('.board-review-selection')).toHaveCount(0);
  await page
    .getByRole('button', { name: 'Analyze position', exact: true })
    .click();
  const native = page.locator('.analysis-panel');
  await native
    .getByRole('button', { name: 'Preview best move', exact: true })
    .click();
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
    await page
      .getByRole('tab', { name: 'Importierte Analyse', exact: true })
      .click();
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
      new URL(r.url()).pathname.endsWith('/moves') &&
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
  const nextError = page.waitForRequest(
    (r) =>
      new URL(r.url()).pathname.endsWith('/moves') &&
      new URL(r.url()).searchParams.get('limit') === '100',
  );
  await page.getByRole('button', { name: /^Next error/ }).click();
  expect(new URL((await nextError).url()).searchParams.get('game_no')).toBe(
    '2',
  );
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
  await page.getByLabel('Analysis scope', { exact: true }).selectOption('game');
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await expect(stats).toContainText('run native GNUbg analysis');
  await expect(stats.locator('.statistics-games-table')).toHaveCount(0);
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
  await page.route('**/api/jobs/*/moves?**', async (route) => {
    if (new URL(route.request().url()).searchParams.get('limit') !== '100')
      return route.continue();
    const response = await route.fetch();
    started();
    await gate;
    await route.fulfill({ response }).catch(() => {});
    finished();
  });
  await page.getByRole('button', { name: /^Next error/ }).click();
  await waiting;
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.board-heading')).toContainText('Move 2');
  release();
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
    await page.evaluate(
      (value) => localStorage.setItem('bgtutor.theme', value),
      theme,
    );
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
