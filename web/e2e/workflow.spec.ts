import { test, expect } from '@playwright/test';

test('real import, search, GNUbg analysis, distribution, themes and mobile layout', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/');
  await expect(page.getByText('GNUbg verbunden')).toBeVisible();
  await page.getByLabel('Partiedatei').setInputFiles({
    name: 'tutorial.mat',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      '7 point match\n\n Game 1\n Tutor Demo : 0                    GNUbg Demo : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n 2) 65: 24/18 18/13                31: 8/5 6/5\n',
    ),
  });
  await expect(
    page.getByRole('img', { name: /Backgammon-Position/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Position analysieren' }).click();
  await expect(page.getByText('BESTER ZUG')).toBeVisible({ timeout: 30_000 });
  await expect(page.locator('.best-move')).toContainText('8/5 6/5');
  await page
    .getByRole('button', { name: 'Dice Distribution', exact: true })
    .click();
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 45_000,
  });
  await expect(page.locator('.distribution-caption')).toContainText(
    '21 Kombinationen',
  );
  await page.getByRole('tab', { name: 'Zugkandidaten' }).click();
  await expect(page.getByText('BESTER ZUG')).toBeVisible();
  await page.getByLabel('Board-Theme').selectOption('walnut');
  await expect(page.locator('.theme-walnut')).toBeVisible();
  await page.getByRole('button', { name: 'Nächster Zug' }).click();
  await expect(page.locator('.board-navigation')).toContainText('8/4 6/4');
  await expect(page.locator('.point-number').first()).toHaveText('12');
  await page.getByRole('button', { name: 'Vorheriger Zug' }).click();
  await page.locator('.position-search summary').click();
  await page
    .getByRole('button', { name: 'Aktuelle Position übernehmen' })
    .click();
  await page.getByRole('button', { name: 'Exakt suchen' }).click();
  await expect(page.locator('.position-results button').first()).toBeVisible();
  await page.getByRole('tab', { name: 'Partiewürfe' }).click();
  await expect(page.locator('.observed')).toContainText(
    '4 aufgezeichnete Würfe',
  );
  await page.getByLabel('Partien durchsuchen').fill('Tutor Demo');
  await expect(page.locator('.match-card')).toHaveCount(1);
  await page.getByRole('tab', { name: 'Zugkandidaten' }).click();
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({
    path: 'test-results/bgtutor-desktop.png',
    fullPage: true,
  });
  await page.reload();
  await expect(page.getByLabel('Board-Theme')).toHaveValue('walnut');
  await page
    .locator('.match-card')
    .filter({ hasText: 'Tutor Demo' })
    .first()
    .click();
  await expect(page.getByText('BESTER ZUG')).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(
    page.getByRole('img', { name: /Backgammon-Position/ }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: 'test-results/bgtutor-mobile.png',
    fullPage: true,
  });
  expect(errors).toEqual([]);
});

test('a long imported match loads the next move page', async ({ page }) => {
  await page.goto('/');
  await page
    .getByLabel('Partiedatei')
    .setInputFiles('../backend/tests/fixtures/WC2014.xg');
  await expect(page.locator('.moves-list button')).toHaveCount(100);
  await page.getByRole('button', { name: 'Weiter im Verlauf' }).click();
  await expect(page.locator('.moves-list button')).toHaveCount(200);
  await expect(page.locator('.board-navigation')).toContainText('101 / 460');
});
