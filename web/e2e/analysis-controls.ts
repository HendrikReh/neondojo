import { expect, type Page } from '@playwright/test';

export async function openMatchCalculation(page: Page) {
  const trigger = page
    .locator('.workspace-analysis-command')
    .getByRole('button', {
      name: /^(Analyze game or match|Spiel oder Match analysieren) …$/,
    });
  await expect(trigger).toBeVisible();
  if ((await trigger.getAttribute('aria-expanded')) === 'false')
    await trigger.click();
}

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
