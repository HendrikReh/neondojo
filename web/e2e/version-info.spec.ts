import { expect, test } from './test';

const gnubgVersion = 'GNU Backgammon 1.08.003 20261108';
const sageModel =
  '5b741451d4f49e69eaa1063fc44f5de67d9e2a528cf9b75f32733136d55b352b';
const currentSageVersion = `Open Sage 2.0.20260907; model ${sageModel}; 2ply cubeful; threads 4; adapter 1`;
const forkRepository = 'HendrikReh/bgsage';
const forkRevision = '5b146c7d86fa965f2185e3871f3c56df030fd76f';
const forkUrl = `https://github.com/${forkRepository}/commit/${forkRevision}`;

test('shows the running app and engine versions under Extras', async ({
  page,
}, testInfo) => {
  let sageVersion: string | null = currentSageVersion;
  let sageFork: { repository: string; revision: string } | undefined = {
    repository: forkRepository,
    revision: forkRevision,
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
                version: gnubgVersion,
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
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Extras', exact: true }).click();
  const versions = page.getByRole('tab', {
    name: 'Versions',
    exact: true,
  });
  const [versionResponse] = await Promise.all([
    page.waitForResponse(
      (response) => new URL(response.url()).pathname === '/api/version',
    ),
    versions.click({ timeout: 4000 }),
  ]);
  expect(versionResponse.ok()).toBe(true);
  const { version: commit } = await versionResponse.json();
  expect(commit).toMatch(/^[a-f\d]{40}$/i);
  const panel = page.getByRole('region', { name: 'Version information' });
  const appRow = panel.locator('dl > div').filter({
    has: page.getByText('BG Tutor', { exact: true }),
  });
  const appCommit = appRow.getByText(commit.slice(0, 7), { exact: true });
  await expect(appCommit).toBeVisible();
  await expect(appCommit).toHaveAttribute('title', commit);
  await expect(panel.getByText('1.08.003', { exact: true })).toBeVisible();
  await expect(panel.getByText('2.0.20260907', { exact: true })).toBeVisible();
  await expect(panel.getByText('Available', { exact: true })).toHaveCount(2);
  await expect(
    panel.getByText(currentSageVersion, { exact: true }),
  ).toBeHidden();

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

  const copy = panel.getByRole('button', { name: 'Copy version information' });
  await copy.focus();
  await copy.press('Enter');
  await expect(panel.getByRole('status')).toHaveText(
    'Version information copied.',
  );
  await expect(copy).toBeFocused();
  const clipboard = await page.evaluate(() => navigator.clipboard.readText());
  for (const detail of [commit, gnubgVersion, currentSageVersion, forkUrl]) {
    expect(clipboard).toContain(detail);
  }

  const details = panel.getByText('Technical details', { exact: true });
  await details.focus();
  await details.press('Enter');
  await expect(panel.getByText(commit, { exact: true })).toBeVisible();
  await expect(panel.getByText(gnubgVersion, { exact: true })).toBeVisible();
  await expect(
    panel.getByText(currentSageVersion, { exact: true }),
  ).toBeVisible();
  const commitLink = panel.getByRole('link', {
    name: `${forkRepository} ${forkRevision}`,
    exact: true,
  });
  await expect(commitLink).toHaveAttribute('href', forkUrl);
  await expect(commitLink).toHaveAttribute('target', '_blank');
  await panel.screenshot({ path: testInfo.outputPath('versions-details.png') });

  await page.getByRole('tab', { name: 'Benchmark', exact: true }).click();
  engineStatus = 503;
  await versions.click();
  await expect(panel.getByRole('alert')).toBeVisible();
  await expect(appCommit).toBeVisible();

  let unexpectedVersionReads = 0;
  await page.route('**/api/version', (route) => {
    unexpectedVersionReads++;
    return route.fulfill({ status: 503, json: { error: 'Build unavailable' } });
  });
  engineStatus = 200;
  sageFork = undefined;
  sageVersion = 'Open Sage 0.8.0 (adapter 1)';
  await panel.getByRole('button', { name: 'Try again' }).click();
  await expect(panel.getByText('0.8.0', { exact: true })).toBeVisible();
  await expect(copy).toBeEnabled();
  await expect(appCommit).toBeVisible();
  expect(unexpectedVersionReads).toBe(0);
  await details.click();
  await expect(panel.getByText(sageVersion, { exact: true })).toBeVisible();
  await expect(panel.getByText('Open Sage fork', { exact: true })).toHaveCount(
    0,
  );

  await page.getByRole('tab', { name: 'Benchmark', exact: true }).click();
  sageVersion = null;
  await page.route('**/api/version', (route) =>
    route.fulfill({ json: { version: '' } }),
  );
  await versions.click();
  await expect(panel.getByText('1.08.003', { exact: true })).toBeVisible();
  await expect(panel.getByText('Unavailable', { exact: true })).toHaveCount(2);
  await expect(panel.getByRole('alert')).toHaveCount(0);

  await page.context().clearPermissions();
  await page.evaluate(() => {
    Object.defineProperty(navigator, 'clipboard', { value: undefined });
  });
  await copy.click();
  await expect(panel.getByRole('status')).toHaveText(
    'Version information could not be copied.',
  );
});
