import { test, expect, type Page } from './test';
import { detail, id as reportId } from './study-report-fixture';
async function fixture(page: Page, available = true) {
  const response = await page.request.post('/api/imports', {
    multipart: {
      file: {
        name: 'analysis-report.sgf',
        mimeType: 'application/octet-stream',
        buffer: Buffer.from(
          `(;GM[6]PW[Report ${crypto.randomUUID()}]PB[GNUbg]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
        ),
      },
    },
  });
  expect(response.status()).toBe(201);
  const { match } = await response.json();
  const metadata = await (
    await page.request.get(`/api/matches/${match.id}/metadata`)
  ).json();
  await page.route(`**/api/matches/${match.id}/metadata`, (r) =>
    r.fulfill({
      json: { ...metadata, study_report_id: available ? reportId : null },
    }),
  );
  let reads = 0;
  const report = {
    ...detail(),
    version: 3,
    phase: 'match_over',
    library_match_id: match.id,
    human_name: 'Hendrik',
    scores: [7, 3],
    game_no: 1,
    match_length: 7,
  };
  await page.route(`**/api/study/reports/${reportId}`, (r) => {
    reads++;
    return r.fulfill({ json: report });
  });
  await page.addInitScript((matchId) => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem(
      'bgtutor.workspace',
      JSON.stringify({ matchId, gameNo: 1 }),
    );
  }, match.id);
  await page.goto('/');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  return { match, report, reads: () => reads };
}
for (const width of [1440, 390])
  test(`saved report is accessible beside the analysis match name at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1000 });
    const f = await fixture(page);
    const heading = page.locator('.analysis-match-header');
    const button = heading.getByRole('button', {
      name: 'Learning review',
      exact: true,
    });
    await expect(button).toBeVisible();
    expect(f.reads()).toBe(0);
    const code = await page.locator('.position-code code').textContent();
    const move = await page.locator('.board-heading').textContent();
    await button.click();
    const dialog = page.getByRole('dialog', {
      name: 'Learning review',
      exact: true,
    });
    await expect(dialog).toBeVisible();
    await expect(dialog.locator('.play-first-pr')).toContainText('4.500');
    expect(f.reads()).toBe(1);
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(button).toBeFocused();
    await expect(page.locator('.position-code code')).toHaveText(code!);
    await expect(page.locator('.board-heading')).toHaveText(move!);
    await button.click();
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: 'Close', exact: true }).click();
    expect(f.reads()).toBe(1);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: test.info().outputPath(`analysis-report-${width}.png`),
      fullPage: true,
    });
  });
test('matches without a saved report have no report action and perform no report reads', async ({
  page,
}) => {
  const f = await fixture(page, false);
  await expect(page.locator('.analysis-match-header')).toHaveAttribute(
    'aria-busy',
    'false',
  );
  await expect(
    page
      .locator('.analysis-match-header')
      .getByRole('button', { name: 'Learning review', exact: true }),
  ).toHaveCount(0);
  expect(f.reads()).toBe(0);
});

test('report loading errors and a mismatched report can be retried without writes', async ({
  page,
}) => {
  const f = await fixture(page);
  let attempts = 0;
  const writes: string[] = [];
  page.on('request', (r) => {
    if (r.method() !== 'GET' && /\/api\/(study|play|jobs)/.test(r.url()))
      writes.push(r.url());
  });
  await page.route(`**/api/study/reports/${reportId}`, (r) => {
    attempts++;
    return attempts === 1
      ? r.fulfill({
          status: 503,
          json: { error: 'Saved report temporarily unavailable' },
        })
      : r.fulfill({
          json: {
            ...f.report,
            id: attempts === 2 ? 'wrong-session' : reportId,
          },
        });
  });
  await page
    .locator('.analysis-match-header')
    .getByRole('button', { name: 'Learning review', exact: true })
    .click();
  const dialog = page.getByRole('dialog', {
    name: 'Learning review',
    exact: true,
  });
  await expect(dialog.getByRole('alert')).toContainText(
    'Saved report temporarily unavailable',
  );
  await dialog.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  await expect(dialog.locator('.play-learning-review')).toHaveCount(0);
  await dialog.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(dialog.locator('.play-first-pr')).toContainText('4.500');
  expect(attempts).toBe(3);
  expect(writes).toEqual([]);
});
test('closing a pending report and leaving analysis discards the late response', async ({
  page,
}) => {
  const f = await fixture(page);
  let start = () => {},
    release = () => {};
  const started = new Promise<void>((r) => (start = r)),
    held = new Promise<void>((r) => (release = r));
  await page.route(`**/api/study/reports/${reportId}`, async (r) => {
    start();
    await held;
    await r.fulfill({ json: f.report }).catch(() => {});
  });
  const button = page
    .locator('.analysis-match-header')
    .getByRole('button', { name: 'Learning review', exact: true });
  await button.click();
  await started;
  const dialog = page.getByRole('dialog', {
    name: 'Learning review',
    exact: true,
  });
  await expect(dialog.getByRole('status')).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  release();
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(button).toBeVisible();
});
