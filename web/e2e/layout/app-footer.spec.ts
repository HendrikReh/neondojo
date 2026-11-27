import { defaultConfiguration } from '../../src/settings/configuration/configuration-model';
import { fulfillJson, mockEngineCatalog } from '../helpers/api-fixtures';
import { profileBackend } from '../helpers/profile-backend';
import { expect, test } from '../helpers/test';

test('short workspaces keep the status bar at the bottom and follow the configured engine', async ({
  page,
}, info) => {
  const configuration = structuredClone(defaultConfiguration);
  configuration.analysis.engine = 'bgsage';
  await profileBackend(page, { configuration });
  let available = true;
  await mockEngineCatalog(page, () => [
    {
      id: 'bgsage',
      name: 'NeonGammon',
      available,
      version: available ? 'NeonGammon 3.2.1' : null,
      plies: [1, 2, 3, 4],
    },
  ]);
  await page.route('**/api/health', (route) =>
    route.fulfill({ json: { status: 'ok' } }),
  );
  await page.goto('/');
  const footer = page.locator('.app-footer');
  await expect(page.locator('main footer')).toHaveCount(0);
  await expect(page.getByRole('contentinfo')).toBeVisible();
  await expect(footer).toContainText('NeonGammon connected');

  // Skip-to-main link is the first keyboard stop, becomes visible on focus,
  // and moves focus into the main landmark when activated.
  await page.keyboard.press('Tab');
  const skip = page.getByRole('link', { name: 'Skip to main content' });
  await expect(skip).toBeFocused();
  await expect(skip).toBeVisible();
  const skipBox = (await skip.boundingBox())!;
  expect(skipBox.width).toBeGreaterThan(80);
  expect(skipBox.height).toBeGreaterThan(20);
  await page.keyboard.press('Enter');
  await expect(page.locator('main#main-content')).toBeFocused();
  const tab = (name: string) => page.getByRole('tab', { name, exact: true });
  for (const [workspace, name] of [
    ['Extras', 'Versions'],
    ['Extras', 'Database backup'],
    ['Study', 'Overview'],
    ['Study', 'Flashcards & Anki'],
  ]) {
    await tab(workspace).click();
    await tab(name).click();
    await expect(
      page.getByRole('tabpanel', { name, exact: true }),
    ).toBeVisible();
    for (const [width, height] of [
      [1800, 1169],
      [2560, 1440],
    ]) {
      await page.setViewportSize({ width, height });
      const inset = await footer.evaluate(
        (element) => innerHeight - element.getBoundingClientRect().bottom,
      );
      expect(inset).toBeGreaterThanOrEqual(0);
      expect(inset).toBeLessThanOrEqual(16);
      await expect(
        footer.getByRole('link', { name: 'OpenAPI ↗' }),
      ).toBeInViewport({ ratio: 1 });
      await page.screenshot({
        fullPage: true,
        path: info.outputPath(`${name}-${width}.png`),
      });
    }
  }
  await tab('Playing').click();
  await expect(footer).toContainText('GNUbg connected');
  available = false;
  await tab('Study').click();
  await expect(footer).toContainText('NeonGammon unavailable');
  await expect(footer.locator('.status-dot')).not.toHaveClass(/ready/);
  await tab('Playing').click();
  await page.route('**/api/engines', (route) =>
    fulfillJson<'ErrorBody'>(route, { error: 'Unavailable' }, 503),
  );
  await tab('Study').click();
  await expect(footer).toContainText('Could not check NeonGammon');
});
