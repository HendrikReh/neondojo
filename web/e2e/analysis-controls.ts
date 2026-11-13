import type { Page } from '@playwright/test';

export async function openPositionCalculation(page: Page, dice = false) {
  const labels = dice
    ? ['Recalculate dice equity', 'Würfel-Equity neu berechnen']
    : ['Recalculate position', 'Position neu berechnen'];
  const dialog = page.getByRole('dialog', {
    name: new RegExp(`^(${labels.join('|')})$`),
  });
  if (!(await dialog.isVisible())) {
    await page
      .getByRole('button', { name: new RegExp(`^(${labels.join('|')}) …$`) })
      .click();
  }
  return dialog;
}
