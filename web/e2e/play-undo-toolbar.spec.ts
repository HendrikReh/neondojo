import { test, expect } from './test';
import {
  historyFixture,
  historySessionId as id,
} from './play-history-review-helpers';
for (const config of [
  { width: 1440, human: 0 },
  { width: 390, human: 1 },
])
  test(`Undo stays beside the board and reverses steps ${config.width}/${config.human}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: config.width, height: 844 });
    const { writes, point } = await historyFixture(page, config.human);
    const toolbar = page.getByTestId('play-board-toolbar');
    const undo = toolbar.getByRole('button', {
      name: 'Undo step',
      exact: true,
    });
    await expect(undo).toBeVisible();
    await expect(undo).toBeDisabled();
    expect(
      await undo.evaluate((el) => el.getBoundingClientRect().height),
    ).toBeGreaterThanOrEqual(44);
    await point(13).click();
    await expect(undo).toBeEnabled();
    const board = (await page
      .locator('.play-board > .board-shell > .board')
      .boundingBox())!;
    const box = (await undo.boundingBox())!;
    expect(box.x).toBeGreaterThanOrEqual(board.x + board.width);
    expect(box.y).toBeLessThan(board.y + board.height);
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(config.width);
    await page.screenshot({
      path: `/tmp/bgtutor-undo-toolbar-${config.width}.png`,
      fullPage: true,
    });
    await undo.click();
    await expect(undo).toBeDisabled();
    await expect(page.locator('.play-selected-steps')).toHaveCount(0);
    await point(13).click();
    await expect(undo).toBeEnabled();
    const undoOffset = async () =>
      (await undo.boundingBox())!.y - (await toolbar.boundingBox())!.y;
    const undoBeforeConfirm = await undoOffset();
    await point(6).click();
    await expect(undo).toBeEnabled();
    await expect(
      page.locator('.play-board [data-dice-action="confirm"]'),
    ).toBeEnabled();
    expect(await undoOffset()).toBeCloseTo(undoBeforeConfirm, 0);
    await undo.click();
    await expect(undo).toBeEnabled();
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    await undo.click();
    await expect(undo).toBeDisabled();
    expect(writes).toEqual([]);
  });
test('Undo remains reachable while scrolling a large board and hides in history review', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 650 });
  const { point, rows, writes } = await historyFixture(page);
  await point(13).click();
  const undo = page
    .getByTestId('play-board-toolbar')
    .getByRole('button', { name: 'Undo step', exact: true });
  await expect(undo).toBeEnabled();
  await page
    .locator('.play-board > .board-shell > .board')
    .evaluate((el) => window.scrollBy(0, el.getBoundingClientRect().top + 120));
  await expect
    .poll(async () => {
      const b = await undo.boundingBox();
      return !!b && b.y >= 0 && b.y + b.height <= 650;
    })
    .toBe(true);
  await undo.click();
  await expect(undo).toBeDisabled();
  await rows.first().click();
  await expect(page.getByTestId('play-board-toolbar')).toHaveCount(0);
  expect(writes).toEqual([]);
});
test('Undo cannot change the position while Coach feedback is pending', async ({
  page,
}) => {
  const { saved, writes } = await historyFixture(page);
  Object.assign(saved, {
    play_mode: 'coach',
    pending_decision: {
      id,
      action: 'move',
      move_index: 0,
      original_action: 'move',
      original_move_index: 0,
      warning: false,
      revealed: true,
      assisted: false,
      retry_count: 0,
      loss: 0,
      first_loss: 0,
      severity: 'none',
      first_severity: 'none',
      alternatives: [],
      attempts: [],
      committed: false,
    },
  });
  await page.reload();
  await page.locator('#workspace-tab-play').click();
  const toolbar = page.getByTestId('play-board-toolbar');
  await expect(toolbar.getByTestId('play-feedback')).toBeVisible();
  await expect(
    toolbar.getByRole('button', { name: 'Undo step', exact: true }),
  ).toHaveCount(0);
  expect(writes).toEqual([]);
});

test('Hint and Undo are stacked to the right of the live board', async ({
  page,
}) => {
  const { saved, writes } = await historyFixture(page);
  Object.assign(saved, { play_mode: 'coach' });
  await page.reload();
  await page.locator('#workspace-tab-play').click();
  const toolbar = page.getByTestId('play-board-toolbar');
  const hint = toolbar.getByRole('button', { name: 'Hint', exact: true });
  const undo = toolbar.getByRole('button', { name: 'Undo step', exact: true });
  const hintBox = (await hint.boundingBox())!,
    undoBox = (await undo.boundingBox())!,
    board = (await page
      .locator('.play-live-board > .board-shell > .board')
      .boundingBox())!;
  expect(hintBox.x).toBeGreaterThanOrEqual(board.x + board.width);
  expect(undoBox.x).toBeGreaterThanOrEqual(board.x + board.width);
  expect(undoBox.y).toBeGreaterThanOrEqual(hintBox.y + hintBox.height);
  expect(writes).toEqual([]);
});
