import { test, expect, type Page, type Locator } from './test';
import {
  historyFixture,
  historySessionId as id,
} from './play-history-review-helpers';
async function resume(page: Page, kind: 'cube' | 'coach') {
  const { saved, writes } = await historyFixture(page);
  Object.assign(saved, {
    phase: kind === 'cube' ? 'double_response' : 'move',
    play_mode: kind === 'coach' ? 'coach' : 'competition',
  });
  if (kind === 'coach')
    Object.assign(saved, {
      pending_decision: {
        id,
        action: 'move',
        original_action: 'move',
        move_index: 0,
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
  if (kind === 'coach')
    await page
      .getByTestId('play-board-toolbar')
      .getByRole('button', { name: 'Details', exact: true })
      .click();
  const dialog = page.getByRole(kind === 'cube' ? 'alertdialog' : 'dialog', {
    name: kind === 'cube' ? 'Cube decision' : 'Details',
    exact: true,
  });
  await expect(dialog).toBeVisible();
  return { dialog, writes };
}
async function drag(page: Page, dialog: Locator, dx: number, dy: number) {
  const title = await dialog.locator('h2').boundingBox();
  await page.mouse.move(
    title!.x + title!.width / 2,
    title!.y + title!.height / 2,
  );
  await page.mouse.down();
  await page.mouse.move(
    title!.x + title!.width / 2 + dx,
    title!.y + title!.height / 2 + dy,
    { steps: 12 },
  );
  await page.mouse.up();
  await expect(dialog).not.toHaveAttribute('data-dragging', 'true');
}
for (const kind of ['cube', 'coach'] as const)
  test(`${kind} dialog can be moved by its title without a game action`, async ({
    page,
  }) => {
    const { dialog, writes } = await resume(page, kind);
    const before = (await dialog.boundingBox())!;
    await drag(page, dialog, -250, 100);
    const after = (await dialog.boundingBox())!;
    expect(after.x - before.x).toBeCloseTo(-250, 0);
    expect(after.y - before.y).toBeCloseTo(100, 0);
    expect(await dialog.evaluate((el) => el.matches(':modal'))).toBe(true);
    if (kind === 'cube') await page.keyboard.press('Escape');
    await expect(dialog).toBeVisible();
    for (let i = 0; i < 8; i++) {
      await page.keyboard.press('Tab');
      expect(
        await dialog.evaluate((el) => el.contains(document.activeElement)),
      ).toBe(true);
    }
    expect(writes).toEqual([]);
    await page.screenshot({
      path: `/tmp/bgtutor-drag-${kind}.png`,
      fullPage: true,
    });
  });
test('moved dialog stays reachable after content changes and viewport resize', async ({
  page,
}) => {
  const { dialog, writes } = await resume(page, 'cube');
  await drag(page, dialog, -1000, -1000);
  let box = (await dialog.boundingBox())!;
  expect(box.x).toBeCloseTo(16, 0);
  expect(box.y).toBeCloseTo(16, 0);
  await dialog.locator('summary').click();
  await expect(
    dialog.getByLabel('Change play mode', { exact: true }),
  ).toBeVisible();
  box = (await dialog.boundingBox())!;
  expect(box.x).toBeCloseTo(16, 0);
  await expect
    .poll(async () => (await dialog.boundingBox())!.y)
    .toBeGreaterThanOrEqual(15);
  await drag(page, dialog, 1000, 1000);
  await page.setViewportSize({ width: 800, height: 850 });
  await expect
    .poll(async () => {
      const b = (await dialog.boundingBox())!;
      return (
        b.x >= 15 && b.y >= 15 && b.x + b.width <= 785 && b.y + b.height <= 835
      );
    })
    .toBe(true);
  await drag(page, dialog, -80, -50);
  expect(await dialog.evaluate((el) => el.matches(':modal'))).toBe(true);
  expect(writes).toEqual([]);
});
test('a moved dialog keeps its location when preferences expand', async ({
  page,
}) => {
  const { dialog } = await resume(page, 'cube');
  await drag(page, dialog, -240, 0);
  const before = (await dialog.boundingBox())!;
  await dialog.locator('summary').click();
  await expect(
    dialog.getByLabel('Change play mode', { exact: true }),
  ).toBeVisible();
  const after = (await dialog.boundingBox())!;
  expect(after.x).toBeCloseTo(before.x, 0);
  expect(after.y + after.height / 2).toBeCloseTo(
    before.y + before.height / 2,
    0,
  );
});
