import { test, expect } from '../helpers/test';
import { historyFixture } from '../helpers/play-history-review-helpers';
test('compact Settings has a Lucide icon and a keyboard/hover tooltip', async ({
  page,
}) => {
  await page.goto('/');
  const settings = page.getByRole('button', { name: 'Settings', exact: true });
  await expect(settings).toHaveAttribute('data-action-icon', 'settings');
  await expect(settings).toHaveAttribute('data-action-compact', 'true');
  await expect(settings.locator('.action-label')).toHaveCount(0);
  await expect(settings.locator('svg.lucide')).toBeVisible();
  await settings.focus();
  const tip = page.getByRole('tooltip');
  await expect(tip).toBeVisible();
  await expect(tip).toHaveText('Settings');
  expect(await tip.evaluate((el) => el.matches(':popover-open'))).toBe(true);
  await page.keyboard.press('Escape');
  await expect(tip).toHaveCount(0);
  await expect(settings).toBeFocused();
  await page.mouse.move(0, 0);
  await settings.hover();
  await expect(page.getByRole('tooltip')).toHaveText('Settings');
  await page.getByRole('tab', { name: 'Analysis', exact: true }).focus();
  await page.mouse.move(0, 0);
  await expect(page.getByRole('tooltip')).toHaveCount(0);
});
test('focus tooltip remains visible above Settings and preserves dialog Escape', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  await expect(dialog).toBeVisible();
  await expect(
    dialog.getByRole('button', { name: 'Close', exact: true }),
  ).toHaveAttribute('data-action-icon', 'close');
  const close = dialog.locator('.settings-close');
  await close.focus();
  const tip = dialog.getByRole('tooltip');
  await expect(tip).toBeVisible();
  expect(await tip.evaluate((el) => el.matches(':popover-open'))).toBe(true);
  const box = (await tip.boundingBox())!;
  expect(
    await page.evaluate(
      ({ x, y }) =>
        document.elementFromPoint(x, y)?.closest('[role="tooltip"]') !== null,
      { x: box.x + box.width / 2, y: box.y + box.height / 2 },
    ),
  ).toBe(true);
  const oldId = await tip.getAttribute('id');
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.locator(`[id="${oldId}"]`)).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: 'Settings', exact: true }),
  ).toBeFocused();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await dialog.locator('.settings-close').click();
  await expect(dialog).toHaveCount(0);
});
test('important game actions retain text beside their Lucide icons', async ({
  page,
}) => {
  const { point, writes } = await historyFixture(page);
  const undo = page.getByRole('button', { name: 'Undo step', exact: true });
  await expect(undo).toHaveAttribute('data-action-icon', 'undo');
  await expect(undo.locator('.action-label')).toHaveText('Undo step');
  await expect(undo.locator('svg.lucide')).toBeVisible();
  await point(13).click();
  await expect(undo).toBeEnabled();
  await undo.focus();
  // The label is visible, so the tooltip only adds the keyboard shortcut.
  await expect(undo.getByRole('tooltip')).toHaveText('Undo step (Z)');
  await undo.click();
  await expect(undo).toBeDisabled();
  expect(writes).toEqual([]);
});

test('hover tooltip can be reached and does not activate its button', async ({
  page,
}) => {
  await page.goto('/');
  await page.getByRole('link', { name: 'NeonDojo home', exact: true }).focus();
  const settings = page.getByRole('button', { name: 'Settings', exact: true });
  await settings.hover();
  const tip = page.getByRole('tooltip');
  await expect(tip).toBeVisible();
  const box = (await tip.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, {
    steps: 16,
  });
  await expect(tip).toBeVisible();
  await tip.click();
  await expect(
    page.getByRole('dialog', { name: 'Settings', exact: true }),
  ).toHaveCount(0);
  await page.mouse.move(0, 0);
  await expect(tip).toHaveCount(0);
});
