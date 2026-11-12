import { expect, test, type Page } from './test';
import { test as nativeTest } from '@playwright/test';

test.use({ actionTimeout: 10000 });

async function importDesktopMatch(page: Page) {
  await page.addInitScript(() => localStorage.setItem('bgtutor.locale', 'en'));
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
    page.getByRole('tab', { name: 'Overview', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  return imported.match;
}

async function openAnalyzedMatch(page: Page) {
  const match = await importDesktopMatch(page);
  await page.getByRole('button', { name: 'Analyze game', exact: true }).click();
  await expect(page.locator('.match-analysis-progress')).toContainText(
    'completed',
  );
  await page.getByRole('button', { name: /Analyze position/ }).click();
  await expect(page.locator('.candidates tbody tr').first()).toBeVisible();
  return match;
}

nativeTest(
  'analysis board resizes with pointer and keyboard, persists and resets',
  async ({ page }) => {
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
    const saved = () =>
      page.request
        .get('/api/configuration')
        .then((r) => r.json())
        .then((c) => c.current_configuration.appearance);
    const savedOverview = () =>
      saved().then((appearance) => appearance.analysis_overview_board_width);
    await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
    await separator.press('End');
    const detailWidth = Number(await separator.getAttribute('aria-valuenow'));
    await expect.poll(saved).toMatchObject({ analysis_board_width: detailWidth });
    await page.getByRole('tab', { name: 'Overview', exact: true }).click();
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
      analysis_board_width: detailWidth,
      analysis_overview_board_width: Math.round(before + 100),
    });
    await page.reload();
    await expect(separator).toHaveAttribute(
      'aria-valuenow',
      String(Math.round(before + 100)),
    );
    await page.setViewportSize({ width: 1200, height: 900 });
    await expect.poll(() => width('.board-panel')).toBeLessThanOrEqual(350);
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
    await separator.press('ArrowRight');
    await expect.poll(savedOverview).toBe(Math.round(before + 10));
    await separator.dblclick();
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
      analysis_board_width: detailWidth,
      analysis_overview_board_width: null,
    });
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await expect(separator).toHaveAttribute('aria-valuenow', String(detailWidth));
    await separator.press('Enter');
    await expect.poll(saved).toMatchObject({
      analysis_board_width: null,
      analysis_overview_board_width: null,
    });
  },
);

test('detail tabs share a larger board preference independent from overview', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await openAnalyzedMatch(page);
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  const board = page.locator('.board-panel .board');
  const move = page.locator('.board-heading');
  await expect(move).toContainText('Move 2');
  const separator = page.getByRole('separator', {
    name: 'Resize analysis board',
    exact: true,
  });
  const width = async () => (await board.boundingBox())!.width;
  let currentWidth = await width();
  for (const name of ['Statistics', 'Errors', 'Notes', 'Study cards']) {
    await page.getByRole('tab', { name, exact: true }).click();
    await expect(separator).toBeVisible();
    await expect.poll(width).toBeCloseTo(currentWidth, 0);
    await separator.press('ArrowRight');
    currentWidth += 10;
    await expect.poll(width).toBeCloseTo(currentWidth, 0);
    await expect(move).toContainText('Move 2');
  }
  const handle = (await separator.boundingBox())!;
  const x = handle.x + handle.width / 2;
  const y = handle.y + handle.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x - 40, y, { steps: 4 });
  await page.mouse.up();
  currentWidth -= 40;
  await expect.poll(width).toBeCloseTo(currentWidth, 0);
  await page.getByRole('tab', { name: 'Overview', exact: true }).click();
  await expect.poll(width).toBeCloseTo(currentWidth, 0);
  await expect(move).toContainText('Move 2');

  const overviewMax = Number(await separator.getAttribute('aria-valuemax'));
  await separator.press('End');
  await expect(separator).toHaveAttribute('aria-valuenow', String(overviewMax));
  const overviewBoard = await width();
  await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
  await expect
    .poll(async () => Number(await separator.getAttribute('aria-valuemax')))
    .toBeGreaterThan(overviewMax);
  await separator.press('End');
  await expect.poll(width).toBeGreaterThan(overviewBoard + 100);
  const largeBoard = await width();
  await page.screenshot({ path: testInfo.outputPath('Statistics.png') });
  await page.getByRole('tab', { name: 'Overview', exact: true }).click();
  await expect(separator).toHaveAttribute('aria-valuenow', String(overviewMax));
  await expect.poll(width).toBeCloseTo(overviewBoard, 0);
  await separator.press('Home');
  await expect(separator).toHaveAttribute('aria-valuenow', '320');
  for (const name of ['Errors', 'Notes', 'Study cards']) {
    await page.getByRole('tab', { name, exact: true }).click();
    await expect.poll(width).toBeCloseTo(largeBoard, 0);
    await expect(move).toContainText('Move 2');
    await page.screenshot({ path: testInfo.outputPath(`${name}.png`) });
  }
  for (const viewport of [
    { width: 2560, height: 1440 },
    { width: 3840, height: 2160 },
  ]) {
    await page.setViewportSize(viewport);
    await expect.poll(width).toBeCloseTo(largeBoard, 0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth))
      .toBeLessThanOrEqual(viewport.width);
    await page.screenshot({
      path: testInfo.outputPath(`Study cards-${viewport.width}.png`),
    });
  }
  await page.getByRole('tab', { name: 'Overview', exact: true }).click();
  await expect(separator).toHaveAttribute('aria-valuenow', '320');
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
    .locator('.moves-list button:has(.move-assessment-marker)')
    .first()
    .focus();
  await expect(page.getByRole('tooltip')).toContainText('Fixture engine');
});

for (const viewport of [
  { width: 1800, height: 1169 },
  { width: 2560, height: 1440 },
  { width: 3840, height: 2160 },
]) {
  test(`desktop overview keeps analysis panes visible at ${viewport.width}x${viewport.height}`, async ({
    page,
  }, testInfo) => {
    await page.setViewportSize(viewport);
    await openAnalyzedMatch(page);
    await expect(
      page.getByRole('tab', { name: 'Overview', exact: true }),
    ).toHaveAttribute('aria-selected', 'true');
    const regions = [
      '.moves-panel',
      '.board-panel',
      '.overview-evaluation',
      '.overview-errors',
    ];
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
    const errorBounds = (await page.locator('.overview-errors').boundingBox())!;
    const stats = page.getByRole('tabpanel', {
      name: 'Statistics',
      exact: true,
    });
    await expect(
      page.getByRole('heading', { name: 'Native analysis statistics' }),
    ).toBeHidden();
    const boardBounds = (await page.locator('.board-panel').boundingBox())!;
    expect(boardBounds.width).toBeLessThanOrEqual(440);
    expect(evaluationBounds.width).toBeGreaterThanOrEqual(1000);
    expect(errorBounds.y).toBeGreaterThanOrEqual(
      evaluationBounds.y + evaluationBounds.height,
    );
    expect(errorBounds.x + errorBounds.width).toBeCloseTo(
      evaluationBounds.x + evaluationBounds.width,
      0,
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
    const errorsOnly = page.locator(
      '.match-analysis-filters input[type=checkbox]',
    );
    await expect(errorsOnly).toBeChecked();
    await page.getByRole('tab', { name: 'Errors', exact: true }).click();
    await expect(errorsOnly).toBeChecked();
    await page.getByRole('tab', { name: 'Overview', exact: true }).click();
    await expect(errorsOnly).toBeChecked();
    await page.screenshot({ path: testInfo.outputPath('overview.png') });
    await page.getByRole('button', { name: 'Next move', exact: true }).click();
    const boardHeading = page.locator('.board-heading');
    await expect(boardHeading).toContainText('Move 2');
    await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
    await expect(
      stats.getByRole('heading', { name: 'Native analysis statistics' }),
    ).toBeVisible();
    await expect(boardHeading).toContainText('Move 2');
    await page.getByRole('tab', { name: 'Overview', exact: true }).click();
    await expect(boardHeading).toContainText('Move 2');
    await expect(stats).toBeHidden();
    await page
      .getByRole('button', { name: 'Previous move', exact: true })
      .click();
    await page.getByRole('tab', { name: 'Statistics', exact: true }).click();
    await stats.locator('[data-statistics-metric="checker"]').first().click();
    await expect(
      page.getByRole('tab', { name: 'Errors', exact: true }),
    ).toHaveAttribute('aria-selected', 'true');
    await expect(
      page.getByLabel('Filter analysis by decision', { exact: true }),
    ).toHaveValue('checker');
    await expect(page.locator('.statistics-summary:visible')).toHaveCount(0);
    await page.getByRole('tab', { name: 'Overview', exact: true }).click();
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
    await page.getByRole('tab', { name: 'Overview', exact: true }).click();
    await expect(page.locator('.candidates tbody tr').first()).toBeVisible();
    await page.getByRole('tab', { name: 'Notes', exact: true }).click();
    await expect(comment).toHaveValue('Desktop draft');
  });
}

test('a background position analysis refreshes its history row after navigating to another move', async ({
  page,
}) => {
  await importDesktopMatch(page);
  const first = page.locator('.moves-list button').first();
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
  await page
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
