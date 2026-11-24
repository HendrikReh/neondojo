import { readFileSync } from 'node:fs';
import {
  openFullMatchAnalysis,
  openPositionCalculation,
  submitFullMatchAnalysis,
} from './analysis-controls';
import { backendTest as nativeTest, expect, test, type Page } from './test';

test.use({ actionTimeout: 10000 });

const localBoardWidths = (page: Page) =>
  page.evaluate(() =>
    JSON.parse(localStorage.getItem('neondojo.layout.analysis.boardWidths.v1') ?? '{}'),
  );

async function importDesktopMatch(page: Page) {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  const importResponse = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/imports' &&
      response.request().method() === 'POST',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'desktop-analysis.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Desktop ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])(;GM[6]MI[length:7][ws:1][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  const imported = await (await importResponse).json();
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  await expect(
    page.getByRole('tab', { name: 'Checker', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  return imported.match;
}

async function openAnalyzedMatch(page: Page) {
  const match = await importDesktopMatch(page);
  const accepted = page.waitForResponse((response) =>
    response.request().method() === 'POST' &&
    new URL(response.url()).pathname === `/api/matches/${match.id}/analysis`,
  );
  await page.getByRole('button', { name: 'Analyze game', exact: true }).click();
  const gameJob = await (await accepted).json();
  await expect(page.locator('.match-analysis-progress')).toContainText(
    'completed',
    { timeout: 20_000 },
  );
  const submitted = page.waitForResponse((response) =>
    response.request().method() === 'POST' &&
    new URL(response.url()).pathname === '/api/jobs',
  );
  await (await openPositionCalculation(page)).getByRole('button', { name: /Analyze position/ }).click();
  const positionJob = await (await submitted).json();
  // The candidates of the game analysis are visible already. Wait for the
  // position job itself, because its progress row moves the board when it goes.
  await expect
    .poll(
      async () =>
        (await (await page.request.get(`/api/jobs/${positionJob.id}`)).json())
          .status,
      { timeout: 30_000 },
    )
    .toBe('completed');
  await expect(page.locator('.analysis-running-job')).toHaveCount(0);
  await expect(page.locator('.candidates tbody tr').first()).toBeVisible({ timeout: 15_000 });
  await page.getByLabel('Displayed analysis', { exact: true }).selectOption(`job:${gameJob.id}`);
  await expect(page.locator('.candidates tbody tr').first()).toBeVisible();
  return match;
}

nativeTest(
  'analysis board resizes with pointer and keyboard, persists and resets',
  async ({ page }, testInfo) => {
    await page.setViewportSize({ width: 1800, height: 1169 });
    await importDesktopMatch(page);
    await expect(page.getByRole('button', { name: /^Profile:/ })).toBeVisible();
    const separator = page.getByRole('separator', {
      name: 'Resize analysis board',
      exact: true,
    });
    await expect(separator).toBeVisible();
    const width = async (selector: string) =>
      (await page.locator(selector).boundingBox())!.width;
    const before = await width('.board-panel');
    const saved = () => localBoardWidths(page);
    const savedOverview = () =>
      saved().then((appearance) => appearance.analysis_overview_board_width);
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await separator.press('End');
    const detailWidth = Number(await separator.getAttribute('aria-valuenow'));
    await expect
      .poll(saved)
      .toMatchObject({ analysis_notes_board_width: detailWidth });
    await page.getByRole('tab', { name: 'Checker', exact: true }).click();
    await expect.poll(() => width('.board-panel')).toBeCloseTo(before, 0);
    const candidateBefore = await width('.overview-evaluation');
    const handle = (await separator.boundingBox())!;
    await page.mouse.move(handle.x + handle.width / 2, handle.y + 100);
    await page.mouse.down();
    await page.mouse.move(handle.x + handle.width / 2 + 100, handle.y + 100, {
      steps: 8,
    });
    await page.mouse.up();
    await expect.poll(() => width('.board-panel')).toBeCloseTo(before + 100, 0);
    expect(await width('.overview-evaluation')).toBeLessThan(candidateBefore);
    await expect.poll(saved).toMatchObject({
      analysis_notes_board_width: detailWidth,
      analysis_overview_board_width: Math.round(before + 100),
    });
    await page.reload();
    await expect(separator).toHaveAttribute(
      'aria-valuenow',
      String(Math.round(before + 100)),
    );
    await page.setViewportSize({ width: 1200, height: 900 });
    await expect.poll(() => width('.board-panel')).toBeLessThan(before + 100);
    expect(await width('.overview-evaluation')).toBeGreaterThanOrEqual(400);
    expect(await savedOverview()).toBe(Math.round(before + 100));
    await page.setViewportSize({ width: 1800, height: 1169 });
    await expect.poll(() => width('.board-panel')).toBeCloseTo(before + 100, 0);
    const moveBeforeKeys = await page.locator('.board-heading').textContent();
    await separator.focus();
    await separator.press('ArrowLeft');
    await expect.poll(savedOverview).toBe(Math.round(before + 90));
    await separator.press('Home');
    await expect(separator).toHaveAttribute('aria-valuenow', '320');
    await separator.press('End');
    expect(await width('.overview-evaluation')).toBeGreaterThanOrEqual(400);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(1800);
    await separator.press('Enter');
    await expect.poll(savedOverview).toBeNull();
    await expect.poll(() => width('.board-panel')).toBeCloseTo(before, 0);
    await separator.press('ArrowLeft');
    const manual = Math.round(before - 10);
    await expect.poll(savedOverview).toBe(manual);
    await page.setViewportSize({ width: 2560, height: 1440 });
    await expect(separator).toHaveAttribute('aria-valuenow', String(manual));
    await expect.poll(() => width('.board-panel')).toBeCloseTo(manual, 0);
    await expect(separator).toHaveAttribute('aria-valuemax', '1680');
    await separator.dblclick();
    await expect(separator).toHaveAttribute('aria-valuenow', '1680');
    await page.screenshot({ path: testInfo.outputPath('maximum-2560.png') });
    await separator.press('Enter');
    await expect.poll(savedOverview).toBeNull();
    await expect.poll(() => width('.board-panel')).toBeGreaterThan(before + 50);
    await page.setViewportSize({ width: 1800, height: 1169 });
    await expect.poll(() => width('.board-panel')).toBeCloseTo(before, 0);
    await separator.press('ArrowRight');
    await expect.poll(savedOverview).toBe(Math.round(before + 10));
    const maxOverviewWidth = Number(
      await separator.getAttribute('aria-valuemax'),
    );
    const artworkBefore = await width('.board-panel .board');
    await separator.dblclick();
    await expect(separator).toHaveAttribute(
      'aria-valuenow',
      String(maxOverviewWidth),
    );
    await expect.poll(savedOverview).toBe(maxOverviewWidth);
    await expect
      .poll(() => width('.board-panel .board'))
      .toBeGreaterThan(artworkBefore);
    await expect(page.locator('.overview-evaluation')).toBeInViewport();
    expect(await width('.overview-evaluation')).toBeGreaterThanOrEqual(400);
    await page.screenshot({ path: testInfo.outputPath('maximum-1800.png') });
    await separator.press('Enter');
    await expect.poll(savedOverview).toBeNull();
    await expect(page.locator('.board-heading')).toHaveText(moveBeforeKeys!);
    const original = (await separator.boundingBox())!;
    await page.mouse.move(original.x + original.width / 2, original.y + 100);
    await page.mouse.down();
    await page.mouse.move(
      original.x + original.width / 2 + 80,
      original.y + 100,
    );
    await separator.press('Escape');
    await page.mouse.up();
    await expect.poll(() => width('.board-panel')).toBeCloseTo(before, 0);
    await expect.poll(saved).toMatchObject({
      analysis_notes_board_width: detailWidth,
      analysis_overview_board_width: null,
    });
    await page.setViewportSize({ width: 3840, height: 2160 });
    await separator.dblclick();
    await expect.poll(savedOverview).toBe(1680);
    await page.screenshot({ path: testInfo.outputPath('maximum-3840.png') });
    await page.reload();
    await expect(separator).toHaveAttribute('aria-valuenow', '1680');
    await separator.press('Enter');
    await expect.poll(savedOverview).toBeNull();
    await page.setViewportSize({ width: 1800, height: 1169 });
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await expect(separator).toHaveAttribute(
      'aria-valuenow',
      String(detailWidth),
    );
    await separator.press('Enter');
    await expect.poll(saved).toMatchObject({
      analysis_notes_board_width: null,
      analysis_overview_board_width: null,
    });
  },
);

test('checker tabs share the compact board and preserve a candidate preview', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'board-checker-comparison.sgf',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[Comparison ${crypto.randomUUID()}]PB[Opponent]MI[length:7];W[31xufe];B[42aesu])`,
    ),
  });
  await page.getByRole('button', { name: 'Analyze game', exact: true }).click();
  await expect(page.locator('.match-analysis-progress')).toContainText(
    'completed',
  );
  const board = page.locator('.board-panel');
  const best = page.locator('.candidate-comparison-table tr[data-candidate-best="true"] .candidate-preview-button');
  await expect(best).toBeVisible();
  await board.getByRole('button', { name: 'Final', exact: true }).click();
  const checkerCounts = () =>
    board
      .locator('[data-point][data-count]')
      .evaluateAll((points) =>
        points.map((point) => [
          point.getAttribute('data-point'),
          point.getAttribute('data-count'),
        ]),
      );
  const original = await checkerCounts();
  await best.click();
  await expect.poll(checkerCounts).not.toEqual(original);
  const preview = await checkerCounts();
  for (const name of ['Statistics', 'Cube', 'Notes', 'Study cards', 'Checker']) {
    await page.getByRole('tab', { name, exact: true }).click();
    await expect(board.locator('.board-shell')).toHaveAttribute('data-preview-mode', 'after');
    await expect.poll(checkerCounts).toEqual(preview);
    await expect(board.locator('.board-checker-comparison')).toHaveCount(0);
    await expect(board.locator('.board-review-help')).toHaveCount(0);
    const navigation = board.locator('.board-navigation');
    await expect(navigation).toBeInViewport();
    expect((await navigation.boundingBox())!.height).toBeLessThanOrEqual(30);
  }
  await board.getByRole('button', { name: 'Reset preview', exact: true }).click();
  await expect.poll(checkerCounts).toEqual(original);
  await expect(board.locator('.board-shell')).toHaveAttribute(
    'data-preview-mode',
    'after',
  );
});

nativeTest('Notes uses wide windows for a larger board and comment editor', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await importDesktopMatch(page);
  const notes = page.getByRole('tab', { name: 'Notes', exact: true });
  const board = page.locator('.board-panel .board');
  await notes.click();
  const pane = page.getByRole('tabpanel', { name: 'Notes', exact: true });
  const editor = pane.getByRole('textbox', { name: 'Comment', exact: true });
  await expect(editor).toBeEditable();
  const initialSize = (await editor.boundingBox())!;
  await editor.fill('Draft in the larger comment editor');

  for (const [width, height] of [[1800, 1169], [2560, 1440], [3840, 2160]]) {
    await page.setViewportSize({ width, height });
    await expect(board).toBeInViewport({ ratio: 0.99 });
    await expect(editor).toBeInViewport({ ratio: 0.99 });
    await expect(editor).toHaveValue('Draft in the larger comment editor');
    expect(await page.evaluate(() => document.documentElement.scrollWidth))
      .toBeLessThanOrEqual(width);
    await page.screenshot({
      path: testInfo.outputPath(`notes-${width}x${height}.png`),
    });
  }

  const boardSize = (await board.boundingBox())!;
  const editorSize = (await editor.boundingBox())!;
  expect(boardSize.width).toBeGreaterThan(1300);
  expect(editorSize.width).toBeGreaterThan(initialSize.width + 900);
  expect(editorSize.height).toBeGreaterThan(900);
  await expect(pane.getByRole('button', { name: 'Save notes' })).toBeInViewport();

  const separator = page.getByRole('separator', { name: 'Resize analysis board', exact: true });
  await separator.dblclick();
  const saved = () => localBoardWidths(page);
  await expect.poll(saved).toMatchObject({
    analysis_notes_board_width: 1680,
    analysis_board_width: null,
    analysis_overview_board_width: null,
  });
  await page.getByRole('tab', { name: 'Study cards', exact: true }).click();
  await expect(separator).toHaveAttribute('aria-valuemax', '1680');
  await separator.press('Home');
  await separator.dblclick();
  await expect(separator).toHaveAttribute('aria-valuenow', '1680');
  await expect.poll(saved).toMatchObject({ analysis_board_width: 1680 });
  await expect(board).toBeInViewport({ ratio: 0.99 });
  await page.screenshot({ path: testInfo.outputPath('cards-maximum.png') });
  await separator.press('Enter');
  await expect.poll(saved).toMatchObject({ analysis_board_width: null });
  await notes.click();
  await expect(separator).toHaveAttribute('aria-valuenow', '1680');
  await expect(editor).toHaveValue('Draft in the larger comment editor');
  await separator.press('Enter');
  await expect.poll(saved).toMatchObject({ analysis_notes_board_width: null });
});

nativeTest('Statistics fits a larger board and readable tables in wide windows', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await importDesktopMatch(page);
  await submitFullMatchAnalysis(page, await openFullMatchAnalysis(page));
  await expect(page.locator('.match-analysis-progress')).toContainText('Match analysis complete');
  const statistics = page.getByRole('tab', { name: 'Statistics', exact: true });
  await statistics.click();
  const pane = page.getByRole('tabpanel', { name: 'Statistics', exact: true });
  const comparison = pane.locator('.statistics-comparison-table');
  await pane.getByText('Compare saved analyses', { exact: true }).click();
  await expect(comparison).toBeVisible();
  const games = pane.locator('.statistics-games-scroll .statistics-games-table');
  await expect(games.locator('tbody tr').first()).toBeVisible();
  const board = page.locator('.board-panel .board');
  const width = async (locator: typeof board) =>
    (await locator.boundingBox())!.width;
  const narrowWidth = await width(board);
  const comparisonWidth = await width(comparison);
  const summaries = pane.locator('.statistics-summary > article');
  await expect(summaries).toHaveCount(2);
  for (const [width, height] of [[1800, 1169], [2560, 1440], [3840, 2160]]) {
    await page.setViewportSize({ width, height });
    await expect(board).toBeInViewport({ ratio: 0.99 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth))
      .toBeLessThanOrEqual(width);
    await page.screenshot({ path: testInfo.outputPath(`statistics-${width}x${height}.png`) });
  }
  await expect.poll(() => width(board)).toBeGreaterThan(narrowWidth * 2);
  expect(await width(comparison)).toBeGreaterThan(comparisonWidth + 500);
  await expect(games.locator('tbody td').first()).toHaveCSS('font-size', '16px');
  const [first, second] = await Promise.all([summaries.first().boundingBox(), summaries.last().boundingBox()]);
  expect(second!.x).toBeGreaterThanOrEqual(first!.x + first!.width);
  expect(second!.y).toBeCloseTo(first!.y, 0);
  await expect(pane).toBeInViewport();

  const separator = page.getByRole('separator', { name: 'Resize analysis board', exact: true });
  await separator.dblclick();
  const saved = () => localBoardWidths(page);
  await expect.poll(saved).toMatchObject({
    analysis_statistics_board_width: 1680,
    analysis_board_width: null,
    analysis_errors_board_width: null,
    analysis_overview_board_width: null,
  });
  await page.reload();
  await statistics.click();
  await expect(separator).toHaveAttribute('aria-valuenow', '1680');
  await separator.press('Enter');
  await expect.poll(saved).toMatchObject({ analysis_statistics_board_width: null });
});

nativeTest('Errors uses wide windows for the board and move history', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  const response = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: 'wide-error-review.xg',
        mimeType: 'application/octet-stream',
        buffer: readFileSync('../backend/tests/fixtures/WC2014.xg'),
      },
    },
  });
  expect(response.ok(), await response.text()).toBe(true);
  const { match } = await response.json();
  await page.addInitScript((matchId) => {
    localStorage.setItem(
      'neondojo.workspace',
      JSON.stringify({ matchId, gameNo: 1 }),
    );
  }, match.id);
  await page.goto('/');
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  const errors = page.getByRole('tabpanel', { name: 'Cube', exact: true });
  await errors.getByRole('checkbox').uncheck();
  const list = page.locator('.analysis-workspace .moves-list');
  await expect(list).toBeVisible();
  const board = page.locator('.board-panel .board');
  const width = async () => (await board.boundingBox())!.width;
  const listHeight = async () => (await list.boundingBox())!.height;
  const narrowWidth = await width();
  const narrowHeight = await listHeight();
  await page.screenshot({ path: testInfo.outputPath('errors-1800x1169.png') });

  await page.setViewportSize({ width: 2560, height: 1440 });
  await expect(board).toBeInViewport({ ratio: 0.99 });
  await expect.poll(listHeight).toBeGreaterThan(narrowHeight + 100);
  await page.screenshot({ path: testInfo.outputPath('errors-2560x1440.png') });

  await page.setViewportSize({ width: 3840, height: 2160 });
  await expect(board).toBeInViewport({ ratio: 0.99 });
  await expect.poll(width).toBeGreaterThan(narrowWidth * 2);
  await expect.poll(listHeight).toBeGreaterThan(1150);
  await expect(page.getByRole('button', { name: 'Next decision', exact: true })).toBeInViewport();
  const bounds = (await list.boundingBox())!;
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(3840);
  expect(bounds.y + bounds.height).toBeLessThanOrEqual(2160);
  await page.screenshot({ path: testInfo.outputPath('errors-3840x2160.png') });

  const separator = page.getByRole('separator', { name: 'Resize analysis board' });
  await separator.dblclick();
  const saved = () => localBoardWidths(page);
  await expect.poll(saved).toMatchObject({
    analysis_errors_board_width: 1680,
    analysis_board_width: null,
    analysis_overview_board_width: null,
  });
  await page.reload();
  await page.getByRole('tab', { name: 'Cube', exact: true }).click();
  await expect(separator).toHaveAttribute('aria-valuenow', '1680');
  await separator.press('Enter');
  await expect.poll(saved).toMatchObject({ analysis_errors_board_width: null });
});

nativeTest('overview keeps the whole board visible when a checker candidate is selected', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1074 });
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  const response = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: 'overview-board-fit.xg',
        mimeType: 'application/octet-stream',
        buffer: readFileSync('../backend/tests/fixtures/WC2014.xg'),
      },
    },
  });
  expect(response.ok(), await response.text()).toBe(true);
  const { match } = await response.json();
  await page.addInitScript((matchId) => {
    localStorage.setItem(
      'neondojo.workspace',
      JSON.stringify({ matchId, gameNo: 7 }),
    );
  }, match.id);
  await page.goto('/');
  await expect(page.getByLabel('Select game', { exact: true })).toHaveValue('7', {
    timeout: 15000,
  });
  await page.locator('.analysis-workspace .move-history-row').filter({
    has: page.locator('.move-no').getByText('7.12', { exact: true }),
  }).click();
  await page.getByRole('button', { name: 'Arrows', exact: true }).click();
  await page
    .locator('.candidate-comparison-table tr[data-candidate-best="true"]')
    .getByRole('button', { name: /^Preview move 1:/ })
    .click();
  await expect(page.locator('.board-review-selection')).toBeVisible();
  const board = page.locator('.board-panel .board');
  const separator = page.getByRole('separator', {
    name: 'Resize analysis board',
  });
  const table = page.locator('.candidate-comparison-table');
  const columnWidth = () => separator.getAttribute('aria-valuenow').then(Number);
  const artworkHeight = async () => (await board.boundingBox())!.height;

  const initialHeight = await artworkHeight();
  let candidateWidth: number | undefined;
  for (const [width, height] of [
    [1800, 1074],
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(board).toBeInViewport({ ratio: 0.99 });
    expect(await artworkHeight()).toBeGreaterThanOrEqual(180);
    const measuredWidth = (await table.boundingBox())!.width;
    if (candidateWidth !== undefined)
      expect(measuredWidth).toBeGreaterThanOrEqual(candidateWidth - 1);
    candidateWidth = measuredWidth;
    await page.screenshot({
      path: testInfo.outputPath(`fitted-overview-${width}x${height}.png`),
    });
  }

  await expect.poll(artworkHeight).toBeGreaterThan(initialHeight + 100);
  const automaticWidth = await columnWidth();
  const manualWidth = automaticWidth - 10;
  await separator.press('ArrowLeft');
  await expect.poll(columnWidth).toBe(manualWidth);
  await expect(page.getByRole('button', { name: 'Arrows', exact: true }))
    .toHaveAttribute('aria-pressed', 'true');
  await page.setViewportSize({ width: 1800, height: 1000 });
  await expect(board).toBeInViewport({ ratio: 0.99 });
  await page.setViewportSize({ width: 3840, height: 2160 });
  await expect.poll(columnWidth).toBe(manualWidth);
  await separator.press('Enter');
  await expect.poll(columnWidth).toBe(automaticWidth);
});

test('move assessments stay scoped while switching games during a delayed response', async ({
  page,
}) => {
  const match = await openAnalyzedMatch(page);
  const positions = (
    await (
      await page.request.get(`/api/positions?match_id=${match.id}&limit=100`)
    ).json()
  ).items;
  const first = positions.find(
    (position: { game_no: number }) => position.game_no === 1,
  );
  const second = positions.filter(
    (position: { game_no: number }) => position.game_no === 2,
  );
  let release = () => {},
    requested = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const started = new Promise<void>((resolve) => {
    requested = resolve;
  });
  const assessment = (
    position_id: string,
    severity: string,
    equity_loss: number,
  ) => ({
    position_id,
    severity,
    equity_loss,
    source: 'imported',
    engine: 'Fixture engine',
    depth: '2-ply',
    depth_plies: 2,
    job_id: null,
  });
  await page.route('**/api/matches/*/move-assessments?*', async (route) => {
    const url = new URL(route.request().url());
    const game = Number(url.searchParams.get('game_no'));
    if (game === 1) {
      requested();
      await gate;
    }
    await route.fulfill({
      json: {
        match_id: match.id,
        game_no: game,
        job_id: url.searchParams.get('job_id'),
        analysis_id: url.searchParams.get('analysis_id'),
        items:
          game === 1
            ? [assessment(first.id, 'very_bad', 0.2)]
            : [
                assessment(second[0].id, 'doubtful', 0.05),
                assessment(second[1].id, 'bad', 0.1),
              ],
      },
    });
  });
  await page.getByLabel('Select game', { exact: true }).selectOption('2');
  await expect(
    page.locator('.moves-list .move-assessment-doubtful'),
  ).toHaveCount(1);
  await expect(page.locator('.moves-list .move-assessment-bad')).toHaveCount(1);
  await page.getByLabel('Select game', { exact: true }).selectOption('1');
  await started;
  await page.getByLabel('Select game', { exact: true }).selectOption('2');
  await expect(
    page.locator('.moves-list .move-assessment-doubtful'),
  ).toHaveCount(1);
  release();
  await page.unrouteAll({ behavior: 'wait' });
  await expect(
    page.locator('.moves-list .move-assessment-very_bad'),
  ).toHaveCount(0);
  await page
    .locator('.moves-list .move-history-row:has(.move-assessment-marker)')
    .first()
    .focus();
  await expect(page.getByRole('tooltip')).toContainText('Fixture engine');
});

for (const viewport of [
  { width: 1800, height: 1169, minBoardWidth: 650, minTableWidth: 670 },
  { width: 2560, height: 1440, minBoardWidth: 780, minTableWidth: 1100 },
  { width: 3840, height: 2160, minBoardWidth: 1350, minTableWidth: 1900 },
]) {
  test(`desktop overview keeps analysis panes visible at ${viewport.width}x${viewport.height}`, async ({
    page,
  }, testInfo) => {
    await page.setViewportSize({
      width: viewport.width,
      height: viewport.height,
    });
    await openAnalyzedMatch(page);
    await expect(
      page.getByRole('tab', { name: 'Checker', exact: true }),
    ).toHaveAttribute('aria-selected', 'true');
    const regions = ['.moves-panel', '.board-panel', '.overview-evaluation'];
    for (const selector of regions) {
      const region = page.locator(`.analysis-workspace ${selector}`);
      await expect(region).toBeVisible();
      const bounds = await region.boundingBox();
      expect(bounds).not.toBeNull();
      expect(bounds!.x).toBeGreaterThanOrEqual(0);
      expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(viewport.width);
      expect(bounds!.y).toBeGreaterThanOrEqual(0);
      expect(bounds!.y + Math.min(bounds!.height, 120)).toBeLessThan(
        viewport.height,
      );
    }
    const evaluationBounds = (await page
      .locator('.overview-evaluation')
      .boundingBox())!;
    const stats = page.getByRole('tabpanel', {
      name: 'Statistics',
      exact: true,
    });
    await expect(
      page.getByRole('heading', { name: 'Analysis statistics', exact: true }),
    ).toBeHidden();
    const board = page.locator('.board-panel .board');
    await expect(board).toBeInViewport({ ratio: 0.99 });
    expect((await board.boundingBox())!.width).toBeGreaterThanOrEqual(
      viewport.minBoardWidth,
    );
    const candidates = page.locator(
      '.overview-evaluation .candidate-comparison-table',
    );
    await expect(candidates).toBeVisible();
    const tableWidth = (await candidates.boundingBox())!.width;
    expect(tableWidth).toBeGreaterThanOrEqual(viewport.minTableWidth);
    expect(tableWidth).toBeLessThan(evaluationBounds.width);
    expect(evaluationBounds.y + evaluationBounds.height).toBeLessThanOrEqual(
      viewport.height,
    );
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(viewport.width);
    if (viewport.width > 2000)
      expect(
        (await page.locator('.analysis-workspace').boundingBox())!.width,
      ).toBeGreaterThan(2000);
    await expect(page.locator('.match-analysis-progress:visible')).toHaveCount(
      1,
    );
    await expect(
      page.locator('.analysis-heading').getByRole('button'),
    ).toHaveCount(0);
    for (const label of ['Initial', 'Arrows', 'Final'])
      await expect(
        page.getByRole('button', { name: label, exact: true }),
      ).toBeVisible();
    await expect(page.locator('.board-review-selection')).toHaveCount(0);
    await expect(page.locator('.board-checker-comparison')).toHaveCount(0);
    await expect(candidates.locator('.candidate-marker-best')).toBeVisible();
    await expect(candidates.locator('.candidate-marker-played')).toBeVisible();
    const moveNavigation = page.locator('.board-navigation');
    await expect(moveNavigation).toBeInViewport();
    const navigationHeight = (await moveNavigation.boundingBox())!.height;
    expect(navigationHeight).toBeLessThanOrEqual(30);
    const errorsOnly = page.locator(
      '.match-analysis-filters input[type=checkbox]',
    );
    const errorFilters = page.getByRole('tabpanel', { name: 'Cube', exact: true });
    const errorNavigation = page
      .locator('.board-panel')
      .getByRole('group', { name: 'Review errors', exact: true });
    await expect(errorFilters).toBeHidden();
    await expect(errorNavigation).toBeHidden();
    await page.getByRole('tab', { name: 'Cube', exact: true }).click();
    await expect(errorFilters).toBeVisible();
    await expect(errorsOnly).toBeChecked();
    for (const label of ['Previous error', 'Next error'])
      await expect(
        errorNavigation.getByRole('button', { name: label, exact: true }),
      ).toBeVisible();
    await page.getByRole('tab', { name: 'Checker', exact: true }).click();
    await expect(errorFilters).toBeHidden();
    await expect(errorNavigation).toBeHidden();
    await page.screenshot({ path: testInfo.outputPath('overview.png') });
    await page.getByRole('button', { name: 'Next move', exact: true }).click();
    const boardHeading = page.locator('.board-heading');
    await expect(boardHeading).toContainText('Move 2');
    await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
    await expect(
      stats.getByRole('heading', { name: 'Analysis statistics', exact: true }),
    ).toBeVisible();
    await expect(errorNavigation).toBeHidden();
    await expect(boardHeading).toContainText('Move 2');
    await page.getByRole('tab', { name: 'Checker', exact: true }).click();
    await expect(boardHeading).toContainText('Move 2');
    await expect(stats).toBeHidden();
    await page
      .getByRole('button', { name: 'Previous move', exact: true })
      .click();
    await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
    await stats.locator('[data-statistics-metric="checker"]').first().click();
    await expect(
      page.getByRole('tab', { name: 'Checker', exact: true }),
    ).toHaveAttribute('aria-selected', 'true');
    await expect(page.getByRole('button', { name: 'Close statistics selection' })).toBeVisible();
    await expect(page.locator('.statistics-summary:visible')).toHaveCount(0);
    await page.getByRole('tab', { name: 'Checker', exact: true }).click();
    await page.locator('.candidate-preview-button').first().click();
    await expect(page.locator('.board-review-selection')).toBeVisible();
    await page
      .getByRole('button', { name: 'Reset preview', exact: true })
      .click();
    await expect(page.locator('.board-review-selection')).toHaveCount(0);
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    const comment = page
      .locator('.annotation-panel')
      .getByLabel('Comment', { exact: true });
    await comment.fill('Desktop draft');
    await page.getByRole('tab', { name: 'Checker', exact: true }).click();
    await expect(page.locator('.candidates tbody tr').first()).toBeVisible();
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await expect(comment).toHaveValue('Desktop draft');
    // The grid leaves room for the footer, so no analysis tab scrolls the page.
    for (const tab of ['Checker', 'Statistics', 'Cube', 'Notes', 'Study cards']) {
      await page.getByRole('tab', { name: tab, exact: true }).click();
      await expect(
        page.getByRole('tab', { name: tab, exact: true }),
      ).toHaveAttribute('aria-selected', 'true');
      await expect
        .poll(() =>
          page.evaluate(
            () => document.documentElement.scrollHeight - window.innerHeight,
          ),
        )
        .toBeLessThanOrEqual(0);
      await expect(page.locator('.app-footer')).toBeInViewport();
    }
  });
}

test('Checker steps through checker errors without opening Statistics', async ({
  page,
}, testInfo) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.goto('/');
  await page
    .getByLabel('Match file', { exact: true })
    .setInputFiles('../backend/tests/fixtures/WC2014.xg');
  const heading = page.locator('.board-heading');
  await expect(heading).toContainText('Game 1');
  await expect(
    page.getByRole('tab', { name: 'Checker', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  const navigation = page
    .locator('.board-panel')
    .getByRole('group', { name: 'Review errors', exact: true });
  await expect(navigation).toContainText('Checker');
  const next = navigation.getByRole('button', {
    name: 'Next error',
    exact: true,
  });
  const previous = navigation.getByRole('button', {
    name: 'Previous error',
    exact: true,
  });
  await expect(next).toHaveAttribute('aria-keyshortcuts', 'E');
  await expect(previous).toHaveAttribute('aria-keyshortcuts', 'Shift+E');
  await expect(next).toHaveAttribute('data-shortcut', 'E');
  const count = navigation.locator('.board-error-count');
  await expect(count).toHaveText(/^Errors: [1-9]/);
  // The statistics review filters stay closed.
  await expect(
    page.getByRole('button', { name: 'Close statistics selection' }),
  ).toHaveCount(0);
  await heading.click();
  await page.keyboard.press('e');
  await expect(count).toHaveText(/^Error 1 of \d+/);
  const first = (await heading.textContent()) ?? '';
  await page.keyboard.press('e');
  await expect(count).toHaveText(/^Error 2 of \d+/);
  await expect(heading).not.toHaveText(first);
  await page.keyboard.press('Shift+E');
  await expect(count).toHaveText(/^Error 1 of \d+/);
  await expect(heading).toHaveText(first);
  await expect(
    page.getByRole('tab', { name: 'Checker', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(page.locator('.board-panel .board')).toBeInViewport({
    ratio: 0.99,
  });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollHeight - window.innerHeight,
    ),
  ).toBeLessThanOrEqual(0);
  await page.screenshot({ path: testInfo.outputPath('checker-errors.png') });
});

test('a background position analysis refreshes its history row after navigating to another move', async ({
  page,
}) => {
  await importDesktopMatch(page);
  const first = page.locator('.moves-list .move-history-row').first();
  await expect(first.locator('.move-assessment-marker')).toHaveCount(0);
  let release = () => {},
    requested = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const started = new Promise<void>((resolve) => {
    requested = resolve;
  });
  await page.route('**/api/jobs/*', async (route) => {
    if (route.request().method() === 'GET') {
      requested();
      await gate;
    }
    await route.continue();
  });
  await (await openPositionCalculation(page))
    .getByRole('button', { name: 'Analyze position', exact: true })
    .click();
  await started;
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.board-heading')).toContainText('Move 2');
  const refresh = page.waitForRequest(
    (request) => new URL(request.url()).pathname.endsWith('/move-assessments'),
    { timeout: 10000 },
  );
  release();
  await refresh;
  await expect(first).toHaveAttribute('data-assessment-source', 'native');
  await expect(first.locator('.move-assessment-marker')).toBeVisible();
  await expect(page.locator('.board-heading')).toContainText('Move 2');
  await page.unrouteAll({ behavior: 'wait' });
});
