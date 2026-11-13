import { expect, type Locator } from '@playwright/test';

export async function showPositionColumns(pane: Locator, ...names: string[]) {
  const picker = pane.locator('.library-column-picker');
  await picker.locator('summary').click();
  for (const name of names)
    await picker.getByRole('checkbox', { name, exact: true }).check();
  await picker.locator('summary').click();
}

/** Keep rows compact while preserving the action icons' 44px click targets. */
export async function expectCompactPositionRow(row: Locator) {
  const remove = row.locator('.position-remove-saved');
  await expect(remove).toBeVisible();
  // Measure together so scroll anchoring cannot move the row between reads.
  const { rowBox, buttonBox, cellBox } = await remove.evaluate((button) => ({
    rowBox: button.closest('tr')!.getBoundingClientRect().toJSON(),
    buttonBox: button.getBoundingClientRect().toJSON(),
    cellBox: button.closest('td')!.getBoundingClientRect().toJSON(),
  }));

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
