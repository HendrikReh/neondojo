import { expect, test } from './test';
import type { SeriesDetails } from '../src/engine-match-api';

test('plays a seeded pair with the default dice mode and exports the return leg', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Extras', exact: true }).click();
  await page.locator('nav').getByText('Engine duels', { exact: true }).click();
  const newSeries = page.getByRole('button', { name: 'New series' });
  const setup = page.getByRole('dialog', { name: 'New series' });
  const archive = page.getByRole('region', { name: 'Series', exact: true });
  const results = page.getByRole('region', { name: 'Series results' });
  await expect(page.getByLabel('Dice mode')).toBeHidden();
  await newSeries.click();
  await expect(setup.getByLabel('Dice mode')).toHaveValue('paired');
  const engineA = setup.getByRole('group', { name: 'Engine A' });
  const engineB = setup.getByRole('group', { name: 'Engine B' });
  await expect(setup.getByRole('button', { name: 'Close' })).toBeFocused();
  await page.keyboard.press('Tab');
  await expect(
    engineA.getByRole('combobox', { name: 'Engine A' }),
  ).toBeFocused();
  await engineA.getByLabel('Playing strength').selectOption('0');
  await engineB.getByLabel('Playing strength').selectOption('0');
  await setup.getByLabel('Match length in points').fill('1');
  await page.keyboard.press('Escape');
  await expect(newSeries).toBeFocused();
  await newSeries.click();
  await expect(setup.getByLabel('Match length in points')).toHaveValue('1');
  await setup.getByRole('spinbutton', { name: 'Match pairs' }).fill('1');
  const seed = '00000000-0000-0000-0000-000000000000';
  const seedInput = setup.getByLabel('Seed (optional UUID)');
  await expect(seedInput).toBeHidden();
  await setup.getByText('Advanced', { exact: true }).click();
  await seedInput.fill(seed);
  await setup.getByRole('button', { name: 'Start series' }).click();
  await expect(setup).toBeHidden();
  await expect(results.getByText('Completed · 2 / 2 Matches')).toBeVisible({
    timeout: 30000,
  });
  await expect(
    results.getByText('A · GNUbg 0-ply', { exact: true }),
  ).toBeVisible();
  await expect(
    results.getByText('B · GNUbg 0-ply', { exact: true }),
  ).toBeVisible();
  await expect(results.getByText('50.0%', { exact: true })).toHaveCount(2);
  await expect(results.getByText('Matches counted')).toBeVisible();
  const pairedSeries = archive
    .getByRole('button', { name: /GNUbg 0-ply.*GNUbg 0-ply.*Completed/ })
    .first();
  await expect(pairedSeries).toHaveAttribute('aria-current', 'true');
  const first = results.getByRole('row', { name: /First leg/ });
  const second = results.getByRole('row', { name: /Return leg/ });
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

  await newSeries.click();
  await setup.getByLabel('Dice mode').selectOption('random');
  await expect(
    setup.getByRole('spinbutton', { name: 'Matches', exact: true }),
  ).toBeVisible();
  await page.keyboard.press('Escape');

  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(
      page.getByRole('heading', { name: 'Engine duels' }),
    ).toBeVisible();
    await page.screenshot({
      path: testInfo.outputPath(`engine-matches-${width}.png`),
    });
  }
  await results
    .getByRole('button', { name: 'Repeat series …' })
    .click();
  await expect(setup.getByLabel('Dice mode')).toHaveValue('paired');
  await expect(seedInput).toBeVisible();
  await expect(seedInput).toHaveValue(seed);
  await setup.getByLabel('Dice mode').selectOption('random');
  await setup.getByRole('button', { name: 'Start series' }).click();
  await expect(results.getByText('Completed · 1 / 1 Matches')).toBeVisible({
    timeout: 30000,
  });
  await expect(
    results.getByRole('row', { name: /First leg|Return leg/ }),
  ).toHaveCount(0);
  await archive
    .getByRole('button', { name: /GNUbg 0-ply.*GNUbg 0-ply.*Completed/ })
    .last()
    .press('Enter');
  await expect(results.getByText('Completed · 2 / 2 Matches')).toBeVisible();
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

test('shows partial results and leaves raw engine errors collapsed', async ({
  page,
}) => {
  const id = '10000000-0000-0000-0000-000000000000';
  const diagnostic = '/local/path/gnubg: engine process exited unexpectedly';
  const details: SeriesDetails = {
    series: {
      id,
      configuration: {
        a: { engine_id: 'gnubg', plies: 3 },
        b: { engine_id: 'bgsage', plies: 4 },
        count: 25,
        match_length: 5,
        mode: 'paired',
        seed: id,
      },
      engines: {
        a: { engine_id: 'gnubg', gnubg_path: '', engine_version: '1' },
        b: { engine_id: 'bgsage', gnubg_path: '', engine_version: '2' },
        rules_version: '1',
      },
      dice_version: 'test',
      created_at: '2026-11-13T12:00:00Z',
      total_matches: 50,
      completed_matches: 41,
      status: 'failed',
      error: diagnostic,
    },
    statistics: {
      complete_pairs: 20,
      wins_a: 24,
      wins_b: 16,
      win_rate_a: 0.6,
      confidence_interval_a: [0.3, 0.9],
    },
    matches: [],
  };
  await page.route('**/api/engine-matches', (route) =>
    route.fulfill({ json: { runs: [details.series] } }),
  );
  await page.route(`**/api/engine-matches/${id}?*`, (route) =>
    route.fulfill({ json: details }),
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Extras', exact: true }).click();
  await page.locator('nav').getByText('Engine duels', { exact: true }).click();
  const results = page.getByRole('region', { name: 'Series results' });
  await expect(
    results.getByText('A · GNUbg 3-ply', { exact: true }),
  ).toBeVisible();
  await expect(
    results.getByText('B · Open Sage 4-ply', { exact: true }),
  ).toBeVisible();
  await expect(results.getByText('60.0%', { exact: true })).toBeVisible();
  await expect(results.getByText('40.0%', { exact: true })).toBeVisible();
  await expect(results.getByText('30.0% – 90.0%')).toBeVisible();
  await expect(results.getByText('20 / 25')).toBeVisible();
  await expect(results.getByText('41 / 50 Matches')).toBeVisible();
  await expect(
    results.getByText(
      'The series stopped before all matches were played. Completed matches are still available.',
    ),
  ).toBeVisible();
  const error = results.getByText(diagnostic);
  await expect(error).toBeHidden();
  await results.getByText('Details', { exact: true }).click();
  await expect(error).toBeVisible();
});

test('shows the retained diagnostic for a cancelled series', async ({
  page,
}) => {
  const id = '20000000-0000-0000-0000-000000000000';
  const side = { engine_id: 'gnubg', plies: 0 };
  const snapshot = { engine_id: 'gnubg', gnubg_path: '' };
  const series = {
    id,
    configuration: {
      a: side,
      b: side,
      count: 1,
      match_length: 1,
      mode: 'paired',
      seed: id,
    },
    engines: { a: snapshot, b: snapshot },
    dice_version: 'test',
    created_at: '2026-11-13T12:00:00Z',
    total_matches: 2,
    completed_matches: 0,
    status: 'cancelled',
    error: 'Interrupted by database restore',
  };
  await page.route('**/api/engine-matches', (route) =>
    route.fulfill({ json: { runs: [series] } }),
  );
  await page.route(`**/api/engine-matches/${id}?*`, (route) =>
    route.fulfill({
      json: {
        series,
        statistics: { complete_pairs: 0, wins_a: 0, wins_b: 0 },
        matches: [],
      },
    }),
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Extras', exact: true }).click();
  await page.locator('nav').getByText('Engine duels', { exact: true }).click();
  const results = page.getByRole('region', { name: 'Series results' });
  await expect(results.getByText('Cancelled · 0 / 2 Matches')).toBeVisible();
  await expect(results.getByText('A · GNUbg 0-ply')).toHaveCount(0);
  await expect(results.getByText('Matches counted')).toHaveCount(0);
  await expect(
    results.getByRole('button', { name: 'Repeat series …' }),
  ).toBeVisible();
  await expect(
    results.getByText('Interrupted by database restore'),
  ).toBeHidden();
  await results.getByText('Details', { exact: true }).click();
  await expect(
    results.getByText('Interrupted by database restore'),
  ).toBeVisible();
});
