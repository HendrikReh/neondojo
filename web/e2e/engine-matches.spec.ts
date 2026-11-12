import { expect, test } from './test';

test('plays a seeded pair with the default dice mode and exports the return leg', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Extras', exact: true }).click();
  await page
    .getByRole('button', { name: 'Engine vs. engine', exact: true })
    .click();
  await expect(page.getByLabel('Dice mode')).toHaveValue('paired');
  const engineA = page.getByRole('group', { name: 'Engine A' });
  const engineB = page.getByRole('group', { name: 'Engine B' });
  await engineA.getByLabel('Playing strength').selectOption('0');
  await engineB.getByLabel('Playing strength').selectOption('0');
  await page.getByLabel('Match length in points').fill('1');
  await page.getByRole('spinbutton', { name: 'Match pairs' }).fill('1');
  const seed = '00000000-0000-0000-0000-000000000000';
  await page.getByLabel('Seed (optional UUID)').fill(seed);
  await page.getByRole('button', { name: 'Start series' }).click();
  await expect(page.getByText('Completed · 2 / 2 Matches')).toBeVisible({
    timeout: 30000,
  });
  await expect(page.getByText('A 50.0% (1) · B 50.0% (1)')).toBeVisible();
  const first = page.getByRole('row', { name: /First leg/ });
  const second = page.getByRole('row', { name: /Return leg/ });
  await expect(
    first.getByRole('cell', { name: 'A', exact: true }),
  ).toBeVisible();
  await expect(
    second.getByRole('cell', { name: 'B', exact: true }),
  ).toBeVisible();
  const path = await second
    .getByRole('link', { name: 'SGF' })
    .getAttribute('href');
  expect(path).toMatch(/\/matches\/1\/sgf$/);
  const exportResponse = await page.request.get(
    new URL(path!, page.url()).href,
  );
  expect(exportResponse.ok()).toBeTruthy();
  expect(await exportResponse.text()).toContain('GNU Backgammon');

  await page.getByLabel('Dice mode').selectOption('random');
  await expect(
    page.getByRole('spinbutton', { name: 'Matches', exact: true }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Reuse settings and seed' }).click();
  await expect(page.getByLabel('Dice mode')).toHaveValue('paired');
  await expect(page.getByLabel('Seed (optional UUID)')).toHaveValue(seed);

  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(
      page.getByRole('heading', { name: 'Engine vs. engine' }),
    ).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath(`engine-matches-${width}.png`),
    });
  }
  await page.getByLabel('Dice mode').selectOption('random');
  await page.getByRole('button', { name: 'Start series' }).click();
  await expect(page.getByText('Completed · 1 / 1 Matches')).toBeVisible({
    timeout: 30000,
  });
  await expect(
    page.getByRole('row', { name: /First leg|Return leg/ }),
  ).toHaveCount(0);
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page
    .locator('.database-sidebar')
    .getByRole('button', { name: /Engine Duels/ })
    .click();
  const rows = page.locator('.database-table tbody tr');
  await expect(rows).toHaveCount(3);
  await expect(rows.first()).toContainText('A gnubg 0-ply');
  await expect(rows.first()).toContainText('B gnubg 0-ply');
});
