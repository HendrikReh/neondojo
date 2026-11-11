import { expect, test, type Page } from '@playwright/test';

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
      '.overview-statistics',
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
    await page
      .locator('.overview-statistics [data-statistics-metric="checker"]')
      .first()
      .click();
    await expect(
      page.getByRole('tab', { name: 'Overview', exact: true }),
    ).toHaveAttribute('aria-selected', 'true');
    await expect(
      page.getByLabel('Filter analysis by decision', { exact: true }),
    ).toHaveValue('checker');
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
