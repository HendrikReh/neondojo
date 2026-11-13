import { expect, test, type Page } from './test';

test.use({ actionTimeout: 10000 });
const header = (page: Page) => page.locator('.analysis-heading');
const field = (page: Page, id: string) =>
  header(page).locator(`[data-match-field="${id}"]`);
async function imported(
  page: Page,
  event = 'Autumn Open',
  initialize = true,
  context: { round: string | null; place: string | null } = {
    round: null,
    place: null,
  },
) {
  if (initialize) {
    await page.addInitScript(() =>
      localStorage.setItem('bgtutor.locale', 'en'),
    );
    await page.goto('/');
  }
  const suffix = crypto.randomUUID();
  const white = `Header ${suffix}`,
    black = 'Opponent';
  const response = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/imports' &&
      r.request().method() === 'POST',
  );
  await page.getByLabel('Match file', { exact: true }).setInputFiles({
    name: `header-${suffix}.sgf`,
    mimeType: 'application/octet-stream',
    buffer: Buffer.from(
      `(;GM[6]PW[${white}]PB[${black}]DT[2026-10-25]EV[${event}]MI[length:21][ws:0][bs:0];W[31hefe];B[42qusu])`,
    ),
  });
  const match = (await (await response).json()).match;
  const metadataResponse = await page.request.put(
    `/api/matches/${match.id}/metadata`,
    {
      data: {
        version: 0,
        display_name: `Header study ${suffix}`,
        played_on: '2026-10-25',
        tournament: event,
        ...context,
        result_state: 'completed',
        white_score: 21,
        black_score: 17,
      },
    },
  );
  expect(metadataResponse.ok()).toBe(true);
  const metadata = await metadataResponse.json();
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  return { match, metadata, white, black, event };
}

test('analysis header shows a compact single-line identity and metadata while database export stays available', async ({
  page,
}) => {
  const event =
    'International Backgammon Championship — Masters division — ' +
    'Autumn '.repeat(17);
  const round = 'Final - Tie Breaker';
  const place =
    'Leonardo Royal Hotel Brighton Waterfront, 6 Kings Road, BN1 2GS Brighton';
  const f = await imported(page, event, true, { round, place });
  await page.reload();
  await expect(field(page, 'id')).toContainText(String(f.metadata.match_no));
  await expect(header(page).getByRole('heading', { level: 1 })).toContainText(
    f.white,
  );
  await expect(header(page).getByRole('heading', { level: 1 })).toContainText(
    f.black,
  );
  await expect(field(page, 'length')).toContainText('21');
  await expect(field(page, 'result')).toContainText('21–17');
  await expect(field(page, 'date')).toContainText('Oct 25, 2026');
  await expect(field(page, 'event')).toContainText(event.trim());
  await expect(field(page, 'round').locator('dd')).toHaveText(round);
  await expect(field(page, 'place').locator('dd')).toHaveText(place);
  await expect(field(page, 'place').locator('dd')).toHaveAttribute(
    'title',
    place,
  );
  await expect(field(page, 'pr-white')).toContainText('n/a');
  await expect(field(page, 'pr-black')).toContainText('n/a');
  await expect(header(page).locator('a[href*="/source"]')).toHaveCount(0);
  await expect(header(page)).not.toContainText('SGF');
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    const title = (await header(page)
      .getByRole('heading', { level: 1 })
      .boundingBox())!;
    for (const id of [
      'id',
      'length',
      'result',
      'date',
      'event',
      'round',
      'place',
      'pr-white',
      'pr-black',
    ]) {
      const box = (await field(page, id).boundingBox())!;
      expect(box.x + box.width).toBeLessThanOrEqual(width);
      if (id === 'round' || id === 'place') {
        const value = (await field(page, id).locator('dd').boundingBox())!;
        expect(value.width).toBeGreaterThan(24);
      }
      expect(
        Math.abs(box.y + box.height / 2 - (title.y + title.height / 2)),
      ).toBeLessThan(18);
    }
    expect((await header(page).boundingBox())!.height).toBeLessThanOrEqual(48);
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
  const titleFont = await header(page)
    .getByRole('heading', { level: 1 })
    .evaluate((el) => parseFloat(getComputedStyle(el).fontSize));
  const metadataFont = await field(page, 'id').evaluate((el) =>
    parseFloat(getComputedStyle(el).fontSize),
  );
  expect(titleFont).toBeGreaterThan(metadataFont);
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.screenshot({
    path: '/tmp/bgtutor-analysis-header-1800.png',
    fullPage: true,
  });
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page
    .getByRole('button', {
      name: `Edit match details: ${f.metadata.display_name}`,
      exact: true,
    })
    .click();
  const dialog = page.getByRole('dialog');
  await dialog.getByLabel('Event', { exact: true }).fill('Updated event');
  await dialog.getByLabel('Round', { exact: true }).fill('');
  await dialog.getByLabel('Location', { exact: true }).fill('Monte-Carlo');
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await expect(field(page, 'event')).toContainText('Updated event');
  await expect(field(page, 'round')).toHaveCount(0);
  await expect(field(page, 'place')).toContainText('Monte-Carlo');
});

test('header PR refreshes after full match completion and stays unknown for a single-game job', async ({
  page,
}) => {
  const f = await imported(page);
  await page.reload();
  await expect(field(page, 'pr-white')).toContainText('n/a');
  const metadataPath = `/api/matches/${f.match.id}/metadata`;
  await page
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('0');
  const gameMetadata = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === metadataPath &&
      r.request().method() === 'GET',
  );
  await page.getByRole('button', { name: 'Analyze game', exact: true }).click();
  await expect(page.locator('.match-analysis-progress')).toContainText(
    'Game analysis complete',
  );
  expect((await (await gameMetadata).json()).white_pr).toBeNull();
  await expect(field(page, 'pr-white')).toContainText('n/a');
  await expect(field(page, 'pr-black')).toContainText('n/a');
  await page
    .getByLabel('Analysis scope', { exact: true })
    .selectOption('match');
  const fullMetadata = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === metadataPath &&
      r.request().method() === 'GET',
  );
  await page
    .getByRole('button', { name: 'Analyze entire match', exact: true })
    .click();
  await expect(page.locator('.match-analysis-progress')).toContainText(
    'Match analysis complete',
  );
  const full = await (await fullMetadata).json();
  expect(full.pr_method).toBe('gnubg_emg_500_v1');
  expect(Number.isFinite(full.white_pr)).toBe(true);
  expect(Number.isFinite(full.black_pr)).toBe(true);
  const pr = (value: number) =>
    new Intl.NumberFormat('en', {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    }).format(value);
  await expect(field(page, 'pr-white')).toContainText(pr(full.white_pr));
  await expect(field(page, 'pr-black')).toContainText(pr(full.black_pr));
  await page.reload();
  await expect(field(page, 'pr-white')).toContainText(pr(full.white_pr));
  await expect(field(page, 'pr-black')).toContainText(pr(full.black_pr));
});

test('metadata failures retain the names and offer retry instead of false unknown data', async ({
  page,
}) => {
  let fail = true;
  await page.route('**/api/matches/*/metadata', (route) => {
    if (route.request().method() === 'GET' && fail) {
      return route.fulfill({
        status: 503,
        json: { error: 'Temporary metadata failure' },
      });
    }
    return route.continue();
  });
  const f = await imported(page);
  await expect(header(page).getByRole('heading', { level: 1 })).toContainText(
    f.white,
  );
  await expect(header(page).getByRole('alert')).toBeVisible();
  await expect(field(page, 'result')).not.toContainText('n/a');
  fail = false;
  await header(page)
    .getByRole('button', { name: /Retry|Try again/ })
    .click();
  await expect(field(page, 'id')).toContainText(String(f.metadata.match_no));
  await expect(field(page, 'result')).toContainText('21–17');
  await expect(header(page).getByRole('alert')).toHaveCount(0);
});

test('a delayed old match response cannot replace the newly opened header', async ({
  page,
}) => {
  let hold = true,
    release = () => {},
    started = () => {},
    finished = () => {};
  const gate = new Promise<void>((r) => {
      release = r;
    }),
    waiting = new Promise<void>((r) => {
      started = r;
    }),
    done = new Promise<void>((r) => {
      finished = r;
    });
  await page.route('**/api/matches/*/metadata', async (route) => {
    if (route.request().method() !== 'GET' || !hold) return route.continue();
    hold = false;
    const response = await route.fetch();
    started();
    await gate;
    await route.fulfill({ response }).catch(() => {});
    finished();
  });
  const first = await imported(page, 'First event');
  await waiting;
  const second = await imported(page, 'Second event', false);
  await expect(header(page)).toHaveAttribute('data-match-id', second.match.id);
  await expect(field(page, 'event')).toContainText('Second event');
  release();
  await done;
  await page.evaluate(
    () =>
      new Promise<void>((r) =>
        requestAnimationFrame(() => requestAnimationFrame(() => r())),
      ),
  );
  await expect(field(page, 'id')).toContainText(
    String(second.metadata.match_no),
  );
  await expect(header(page).getByRole('heading', { level: 1 })).toContainText(
    second.white,
  );
  await expect(header(page)).not.toContainText(first.white);
  await expect(field(page, 'event')).toContainText('Second event');
});
