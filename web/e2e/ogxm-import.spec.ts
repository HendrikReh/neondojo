import { expect, test } from './test';

test('the match file picker imports an OGXM archive match', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.goto('/');
  await expect(page.locator('.match-drop-zone-formats')).toContainText('OGXM');
  const picker = page.locator('input[type=file][accept*=".ogxm"]');
  const imported = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      new URL(response.url()).pathname === '/api/imports',
  );
  await picker.setInputFiles('../backend/tests/fixtures/hedgehog-cdf.ogxm');
  const response = await imported;
  expect(response.status()).toBe(201);
  const { match } = await response.json();
  expect(match).toMatchObject({ format: 'ogxm', match_length: 9 });
  await expect(page.locator('.board-heading')).toBeVisible();
});
