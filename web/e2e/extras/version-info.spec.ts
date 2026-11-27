import { DESKTOP_VIEWPORTS, gotoWorkspace } from '../helpers/navigation';
import { fulfillJson, fulfillJsonList } from '../helpers/api-fixtures';
import { expect, test } from '../helpers/test';

const gnubgVersion = 'GNU Backgammon 1.08.003 20261108';
const sageModel =
  '5b741451d4f49e69eaa1063fc44f5de67d9e2a528cf9b75f32733136d55b352b';
const currentSageVersion = `NeonGammon 3.0.0; model ${sageModel}; 2ply cubeful; threads 4; adapter 1`;
const forkRepository = 'HendrikReh/neongammon';
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
    engineStatus === 200
      ? fulfillJsonList<'EngineInfo'>(route, [
          {
            id: 'gnubg',
            name: 'GNUbg',
            available: true,
            version: gnubgVersion,
            plies: [1, 2, 3, 4, 5],
            max_parallel_threads: 7,
          },
          {
            id: 'bgsage',
            name: 'NeonGammon',
            available: sageVersion !== null,
            version: sageVersion,
            fork: sageFork,
            plies: [2],
            max_parallel_threads: 9,
          },
        ])
      : fulfillJson<'ErrorBody'>(
          route,
          { error: 'Engine catalog unavailable' },
          engineStatus,
        ),
  );
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.setViewportSize({ width: 1800, height: 1169 });
  await gotoWorkspace(page, 'extras');
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
  const { release, commit, version } = await versionResponse.json();
  expect(release).toMatch(/^\d+\.\d+\.\d+$/);
  expect(commit).toMatch(/^[a-f\d]{40}$/i);
  expect(version).toBe(commit);
  const panel = page.getByRole('region', { name: 'Version information' });
  const appRow = panel.locator('dl > div').filter({
    has: page.getByText('NeonDojo', { exact: true }),
  });
  const appCommit = appRow.getByText(`${release} (${commit.slice(0, 7)})`, {
    exact: true,
  });
  await expect(appCommit).toBeVisible();
  await expect(appCommit).toHaveAttribute('title', commit);
  await expect(panel.getByText('1.08.003', { exact: true })).toBeVisible();
  await expect(panel.getByText('3.0.0', { exact: true })).toBeVisible();
  await expect(panel.getByText('Available', { exact: true })).toHaveCount(2);
  await expect(
    panel.getByText(currentSageVersion, { exact: true }),
  ).toBeHidden();

  for (const { width, height } of DESKTOP_VIEWPORTS) {
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
  for (const detail of [
    `NeonDojo version: ${release}`,
    `NeonDojo commit: ${commit}`,
    gnubgVersion,
    currentSageVersion,
    forkUrl,
  ]) {
    expect(clipboard).toContain(detail);
  }

  const details = panel.getByText('Technical details', { exact: true });
  await details.focus();
  await details.press('Enter');
  await expect(panel.getByText(commit, { exact: true })).toBeVisible();
  await expect(panel.getByText(release, { exact: true })).toBeVisible();
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
  sageVersion = 'NeonGammon 0.8.0 (adapter 1)';
  await panel.getByRole('button', { name: 'Try again' }).click();
  await expect(panel.getByText('0.8.0', { exact: true })).toBeVisible();
  await expect(copy).toBeEnabled();
  await expect(appCommit).toBeVisible();
  expect(unexpectedVersionReads).toBe(0);
  await details.click();
  await expect(panel.getByText(sageVersion, { exact: true })).toBeVisible();
  await expect(
    panel.getByText('NeonGammon source', { exact: true }),
  ).toHaveCount(0);

  await page.getByRole('tab', { name: 'Benchmark', exact: true }).click();
  sageVersion = null;
  await page.route('**/api/version', (route) =>
    route.fulfill({ json: { release, commit: '', version: '' } }),
  );
  await versions.click();
  await expect(panel.getByText('1.08.003', { exact: true })).toBeVisible();
  await expect(appRow.getByText(release, { exact: true })).toBeVisible();
  await expect(panel.getByText('Unavailable', { exact: true })).toHaveCount(1);
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
