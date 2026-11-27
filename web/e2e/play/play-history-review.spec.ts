import { DESKTOP_VIEWPORTS, openWorkspace } from '../helpers/navigation';
import { test, expect } from '../helpers/test';
import { historyFixture } from '../helpers/play-history-review-helpers';

test('wide Playing layout enlarges the board and keeps a long history scrollable', async ({
  page,
}, testInfo) => {
  const { saved, rows } = await historyFixture(page);
  const firstMoves = [...saved.history];
  while (saved.history.length < 80) saved.history.push(...firstMoves);
  await page.reload();
  await openWorkspace(page, 'play');

  const board = page.locator('.play-live-board > .board-shell > .board');
  const history = page.locator('.play-history');
  const list = history.locator('.moves-list');
  await expect(rows).toHaveCount(80);
  for (const { width, height } of DESKTOP_VIEWPORTS) {
    await page.setViewportSize({ width, height });
    await expect(board).toBeInViewport({ ratio: 0.99 });
    await expect(history).toBeInViewport({ ratio: 0.99 });
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(width);
    await page.screenshot({
      path: testInfo.outputPath(`playing-${width}x${height}.png`),
    });
  }

  expect((await board.boundingBox())!.width).toBeGreaterThan(2050);
  expect((await history.boundingBox())!.width).toBeGreaterThan(425);
  expect((await list.boundingBox())!.height).toBeGreaterThan(1200);
  await expect(rows.last()).toHaveAttribute('aria-current', 'step');
  await expect(rows.last().locator('strong')).toHaveCSS('font-size', '16px');
  await rows.last().scrollIntoViewIfNeeded();
  await expect(rows.last()).toBeInViewport();
  await expect(history.locator('.play-history-actions')).toBeInViewport();
  await rows.first().scrollIntoViewIfNeeded();
  await expect(rows.first()).toBeInViewport();
  await expect(board).toBeInViewport({ ratio: 0.99 });
});

test('move-history titles reveal only clipped labels', async ({ page }) => {
  const { rows } = await historyFixture(page);
  const row = rows.first();
  const tooltip = page.getByRole('tooltip');
  await row.focus();
  await expect(tooltip).toHaveText('Not evaluated');
  await row.blur();
  await expect(tooltip).toHaveCount(0);
  for (const label of ['strong', '.move-history-player']) {
    await row.locator(label).evaluate((element) => {
      element.style.maxWidth = '16px';
    });
  }
  await row.focus();
  await expect(tooltip).toHaveText('8/5 6/5 · Human · Not evaluated');
  await page.keyboard.press('Escape');
  await expect(tooltip).toHaveCount(0);
  await expect(row).toBeFocused();
  await row.blur();
  await row.hover();
  await expect(tooltip).toHaveText('8/5 6/5 · Human · Not evaluated');
});

test('board dice show their tooltip on focus without moving keyboard focus', async ({
  page,
}) => {
  const { writes } = await historyFixture(page);
  const pair = page.locator('[data-dice-action="swap"]');
  await pair.focus();
  const tooltip = page.getByRole('tooltip');
  await expect(tooltip).toHaveText(/Swap dice/);
  await expect(pair).toHaveAttribute(
    'aria-describedby',
    (await tooltip.getAttribute('id'))!,
  );
  await page.keyboard.press('Escape');
  await expect(tooltip).toHaveCount(0);
  await expect(pair).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(pair).not.toBeFocused();
  expect(writes).toEqual([]);
});

for (const human of [0, 1])
  test(`history mouse/keys, exact partial-turn restore and no live writes ${human}`, async ({
    page,
  }) => {
    const { writes, readCount, point } = await historyFixture(page, human);
    await point(13).click();
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    await expect(page.locator('.play-workspace')).toHaveAttribute(
      'aria-busy',
      'false',
    );
    await expect(page.locator('[data-animated-checker]')).toHaveCount(0);
    const board = page.locator('.play-live-board > .board-shell > .board');
    const liveBoard = await board.boundingBox();
    const rows = page.locator('.play-history .move-history-row');
    await expect(rows).toHaveCount(2);
    await rows.first().click();
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-review',
      '0',
    );
    await expect(page.locator('.play-history-review')).toBeVisible();
    await expect(
      page.locator('.play-history-review [role="status"]'),
    ).toHaveCount(0);
    await expect(page.locator('.board-shell .move-arrows')).toBeVisible();
    expect(await board.boundingBox()).toEqual(liveBoard);
    await expect(
      page.locator('[data-play-point][data-enabled="true"]'),
    ).toHaveCount(0);
    await expect(page.locator('.play-board [data-cube]')).toHaveCSS(
      'pointer-events',
      'none',
    );
    await page.keyboard.press('ArrowRight');
    await expect(page.locator('.board-shell')).toHaveAttribute(
      'data-preview-mode',
      'after',
    );
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-review',
      '1',
    );
    await expect(page.locator('.board-shell')).toHaveAttribute(
      'data-preview-mode',
      'after',
    );
    await page.keyboard.press('ArrowLeft');
    await expect(page.locator('.board-shell')).toHaveAttribute(
      'data-preview-mode',
      'before',
    );
    await page.keyboard.press('ArrowUp');
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-review',
      '0',
    );
    await expect(page.locator('.board-shell')).toHaveAttribute(
      'data-preview-mode',
      'before',
    );
    await page
      .getByRole('button', { name: 'Return to game', exact: true })
      .click();
    await expect(page.locator('.play-history-review')).toHaveCount(0);
    expect(await board.boundingBox()).toEqual(liveBoard);
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    await point(6).click();
    await expect(page.locator('[data-dice-action="confirm"]')).toHaveCount(1);
    await rows.last().click();
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-review',
      '1',
    );
    expect(readCount()).toBe(1);
    expect(writes).toEqual([]);
    await page
      .getByRole('button', { name: 'Return to game', exact: true })
      .click();
    await expect(page.locator('[data-dice-action="confirm"]')).toHaveCount(1);
  });
