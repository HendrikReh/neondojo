import { test, expect } from './test';
import { historyFixture } from './play-history-review-helpers';

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
  const first = page.locator('[data-dice-action="swap"]').first();
  await first.focus();
  const tooltip = page.getByRole('tooltip');
  await expect(tooltip).toHaveText(/Swap dice/);
  await expect(first).toHaveAttribute(
    'aria-describedby',
    (await tooltip.getAttribute('id'))!,
  );
  await page.keyboard.press('Tab');
  const second = page.locator('[data-dice-action="swap"]').last();
  await expect(second).toBeFocused();
  await expect(tooltip).toHaveText(/Swap dice/);
  await page.keyboard.press('Escape');
  await expect(tooltip).toHaveCount(0);
  await expect(second).toBeFocused();
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
    await expect(page.locator('.play-board [data-cube]')).toHaveAttribute(
      'aria-disabled',
      'true',
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
    await expect(page.locator('[data-dice-action="confirm"]')).toHaveCount(2);
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
    await expect(page.locator('[data-dice-action="confirm"]')).toHaveCount(2);
  });
