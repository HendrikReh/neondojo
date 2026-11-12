import { expect, test } from './test';

test('shows the running app and engine versions under Extras', async ({
  page,
}, testInfo) => {
  let sageVersion: string | null = 'Open Sage 0.8.0 (adapter 1)';
  let sageFork: { repository: string; revision: string } | undefined = {
    repository: 'HendrikReh/bgsage',
    revision: '5b146c7d86fa965f2185e3871f3c56df030fd76f',
  };
  let engineStatus = 200;
  await page.route('**/api/engines', (route) =>
    route.fulfill({
      status: engineStatus,
      json:
        engineStatus === 200
          ? [
              {
                id: 'gnubg',
                name: 'GNUbg',
                available: true,
                version: 'GNU Backgammon 1.08.003',
                plies: [0, 1, 2, 3, 4],
              },
              {
                id: 'bgsage',
                name: 'Open Sage',
                available: sageVersion !== null,
                version: sageVersion,
                fork: sageFork,
                plies: [2],
              },
            ]
          : { error: 'Engine catalog unavailable' },
    }),
  );
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Extras', exact: true }).click();
  const versions = page.getByRole('button', {
    name: 'Version information',
    exact: true,
  });
  await versions.click({ timeout: 4000 });
  const panel = page.getByRole('region', { name: 'Version information' });
  await expect(panel.getByText('BG Tutor', { exact: true })).toBeVisible();
  await expect(panel.getByText(/^\d+\.\d+\.\d+/)).toBeVisible();
  await expect(panel.getByText('GNU Backgammon 1.08.003')).toBeVisible();
  await expect(panel.getByText('Open Sage 0.8.0 (adapter 1)')).toBeVisible();
  const fork = panel.getByText('Open Sage fork · HendrikReh/bgsage');
  await expect(fork).toBeVisible();
  await expect(fork.getByText('5b146c7', { exact: true })).toBeVisible();
  const commitLink = fork.getByRole('link', {
    name: 'HendrikReh/bgsage 5b146c7',
    exact: true,
  });
  await expect(commitLink).toHaveAttribute(
    'href',
    'https://github.com/HendrikReh/bgsage/commit/5b146c7d86fa965f2185e3871f3c56df030fd76f',
  );
  await expect(commitLink).toHaveAttribute('target', '_blank');

  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await panel.screenshot({
      path: testInfo.outputPath(`versions-${width}.png`),
    });
  }

  await page.getByRole('button', { name: 'Benchmark', exact: true }).click();
  engineStatus = 503;
  await versions.click();
  await expect(panel.getByRole('alert')).toBeVisible();
  await expect(panel.getByText(/^\d+\.\d+\.\d+/)).toBeVisible();

  engineStatus = 200;
  sageFork = undefined;
  await panel.getByRole('button', { name: 'Try again' }).click();
  await expect(panel.getByText('Open Sage 0.8.0 (adapter 1)')).toBeVisible();
  await expect(fork).toHaveCount(0);

  await page.getByRole('button', { name: 'Benchmark', exact: true }).click();
  sageVersion = null;
  await versions.click();
  await expect(panel.getByRole('alert')).toHaveCount(0);
  await expect(panel.getByText('GNU Backgammon 1.08.003')).toBeVisible();
  await expect(panel.getByText('Unavailable', { exact: true })).toBeVisible();
});
