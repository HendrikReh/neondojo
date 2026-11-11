import { expect, type Locator } from '@playwright/test';

/** Keep rows compact while preserving the action icons' 44px click targets. */
export async function expectCompactPositionRow(row: Locator) {
  const remove = row.locator('.position-remove-saved');
  await expect(remove).toBeVisible();
  const rowBox = (await row.boundingBox())!;
  const buttonBox = (await remove.boundingBox())!;
  const cellBox = (await row
    .locator('td:has(.position-remove-saved)')
    .boundingBox())!;

  expect(buttonBox.width).toBeGreaterThanOrEqual(44);
  expect(buttonBox.height).toBeGreaterThanOrEqual(44);
  // 44px target, 5px cell padding on each side, a border, and rounding tolerance.
  expect(rowBox.height).toBeLessThanOrEqual(56);
  expect(buttonBox.y).toBeGreaterThanOrEqual(rowBox.y);
  expect(buttonBox.y + buttonBox.height).toBeLessThanOrEqual(
    rowBox.y + rowBox.height,
  );
  expect(buttonBox.x).toBeGreaterThanOrEqual(cellBox.x);
  expect(buttonBox.x + buttonBox.width).toBeLessThanOrEqual(
    cellBox.x + cellBox.width,
  );
}
