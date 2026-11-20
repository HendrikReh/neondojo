import { profileBackend } from './profile-backend';
import { defaultConfiguration } from '../src/configuration-model';
import { expect, test } from './test';

test.describe.configure({ timeout: 180_000 });

test('a running dice job uses one event stream and no full read per progress event', async ({
  page,
}) => {
  const configuration = structuredClone(defaultConfiguration);
  // 3-ply sends 21 progress events over about two seconds.
  configuration.analysis.plies = 3;
  await profileBackend(page, { configuration });
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  const streams: string[] = [];
  const reads: string[] = [];
  page.on('request', (request) => {
    const path = new URL(request.url()).pathname;
    if (/^\/api\/jobs\/[^/]+\/events$/.test(path)) streams.push(path);
    else if (request.method() === 'GET' && /^\/api\/jobs\/[^/]+$/.test(path))
      reads.push(path);
  });
  await page.goto('/');
  await expect(
    page.getByText('GNUbg connected', { exact: true }),
  ).toBeVisible();
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: 'job-watch.mat',
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `7 point match\n\n Game 1\n Watch ${crypto.randomUUID()} : 0       Opponent : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n`,
    ),
  });
  await expect(page.locator('.analysis-panel')).toBeVisible();
  const submitted = page.waitForResponse(
    (response) =>
      response.request().method() === 'POST' &&
      new URL(response.url()).pathname === '/api/jobs',
  );
  await page.getByRole('tab', { name: 'Dice equity', exact: true }).click();
  const job = await (await submitted).json();
  await expect(page.locator('.dice-grid button')).toHaveCount(36, {
    timeout: 120_000,
  });
  const events = await (
    await page.request.get(`/api/jobs/${job.id}/events`)
  ).text();
  const progress = events.match(/^event: progress$/gm)?.length ?? 0;
  const stream = `/api/jobs/${job.id}/events`;
  const read = `/api/jobs/${job.id}`;
  // Let late readers and streams of the finished job settle.
  await page.waitForTimeout(4_000);
  console.log(
    `job-watch: progress events=${progress}, streams=${streams.filter((path) => path === stream).length}, full reads=${reads.filter((path) => path === read).length}`,
  );
  expect(progress).toBeGreaterThanOrEqual(10);
  expect(streams.filter((path) => path === stream)).toHaveLength(1);
  expect(reads.filter((path) => path === read).length).toBeLessThanOrEqual(3);
});
