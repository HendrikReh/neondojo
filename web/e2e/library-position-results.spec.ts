import { expect, test, type Page } from '@playwright/test';

test.use({ actionTimeout: 10000 });
async function fixture(page: Page, count = 2) {
  const suffix = crypto.randomUUID();
  const database = await (
    await page.request.post('/api/databases', {
      data: { name: `Position results ${suffix}` },
    })
  ).json();
  const entries = [];
  for (let n = 0; n <= count; n++) {
    const white = `Search ${n} ${suffix}`,
      hit = n < count;
    const game = `(;GM[6]PW[${white}]PB[Opponent]MI[length:7][ws:0][bs:0];W[${hit ? '31hefe' : '42hdfd'}];B[${hit ? '42qusu' : '31qtst'}])`;
    const response = await page.request.post(
      `/api/imports?database_id=${database.id}`,
      {
        multipart: {
          file: {
            name: `positions-${n}.sgf`,
            mimeType: 'application/octet-stream',
            buffer: Buffer.from(n === 0 ? game + game : game),
          },
        },
      },
    );
    expect(response.status()).toBe(201);
    const match = (await response.json()).match;
    const positions = (
      await (
        await page.request.get(`/api/positions?match_id=${match.id}&limit=100`)
      ).json()
    ).items;
    const metadata = await (
      await page.request.get(`/api/matches/${match.id}/metadata`)
    ).json();
    entries.push({ match, positions, metadata });
  }
  await page.addInitScript((id) => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.database', id);
  }, database.id);
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  return { database, entries, needle: entries[0].positions[1].position_id };
}
async function search(page: Page, id: string) {
  const form = page.locator('.position-search');
  if (!(await form.evaluate((el) => (el as HTMLDetailsElement).open)))
    await form.locator('summary').click();
  await form.getByRole('textbox').fill(id);
  await form
    .getByRole('button', { name: 'Find exact match', exact: true })
    .click();
}
const rows = (page: Page) => page.locator('.database-table tbody tr');

test('position matches use the common table, preserve other filters and open the exact occurrence', async ({
  page,
}) => {
  const f = await fixture(page);
  await expect(rows(page)).toHaveCount(3);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(2);
  await expect(page.locator('.position-results')).toHaveCount(0);
  await expect(
    page.locator('.database-table th[data-column="id"]').getByRole('separator'),
  ).toBeVisible();
  await expect(page.locator('.database-table')).not.toContainText(
    f.entries[2].match.white,
  );
  const filtered = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      new URL(r.url()).searchParams.get('sort') === 'id',
  );
  await page.locator('.database-table button[data-sort="id"]').click();
  expect(new URL((await filtered).url()).searchParams.get('position_id')).toBe(
    f.needle,
  );
  await expect(rows(page).first().locator('.library-id-cell')).toHaveText(
    String(f.entries[0].metadata.match_no),
  );
  await rows(page).first().getByRole('checkbox').check();
  await expect(page.locator('.library-batch-bar')).toBeVisible();
  await page
    .getByLabel('Search matches', { exact: true })
    .fill(f.entries[0].match.white);
  await expect(rows(page)).toHaveCount(1);
  await page
    .getByRole('button', { name: 'Clear position filter', exact: true })
    .click();
  await expect(page.getByLabel('Search matches', { exact: true })).toHaveValue(
    f.entries[0].match.white,
  );
  await expect(rows(page)).toHaveCount(1);
  await expect(page.locator('.position-search input')).toHaveValue('');
  await page.getByLabel('Search matches', { exact: true }).fill('');
  await expect(rows(page)).toHaveCount(3);
  await search(page, 'invalid');
  await expect(page.locator('.position-search [role=alert]')).toBeVisible();
  await expect(rows(page)).toHaveCount(3);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(2);
  for (const mode of ['button', 'enter', 'double']) {
    const first = rows(page).first();
    if (mode === 'button') await first.locator('.match-card').click();
    else if (mode === 'enter') {
      await first.focus();
      await page.keyboard.press('Enter');
    } else await first.locator('td').nth(7).dblclick();
    await expect(page.locator('.analysis-heading')).toHaveAttribute(
      'data-match-id',
      f.entries[0].match.id,
    );
    await expect(page.locator('.board-heading')).toContainText('Move 2');
    await expect(page.locator('.position-code code')).toHaveText(f.needle);
    await page
      .getByRole('tab', { name: 'Match database', exact: true })
      .click();
    await expect(rows(page)).toHaveCount(2);
  }
});

test('late position lookups cannot override a newer row click or changed table filters', async ({
  page,
}) => {
  const f = await fixture(page);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(2);
  await page.locator('.database-table button[data-sort="id"]').click();
  await expect(rows(page).first().locator('.library-id-cell')).toHaveText(
    String(f.entries[0].metadata.match_no),
  );
  async function gate() {
    let release = () => {},
      started = () => {},
      finished = () => {};
    const held = new Promise<void>((r) => {
        release = r;
      }),
      waiting = new Promise<void>((r) => {
        started = r;
      }),
      done = new Promise<void>((r) => {
        finished = r;
      });
    await page.route('**/api/positions?**', async (route) => {
      const q = new URL(route.request().url()).searchParams;
      if (
        q.get('match_id') !== f.entries[0].match.id ||
        q.get('position_id') !== f.needle
      )
        return route.continue();
      const response = await route.fetch();
      started();
      await held;
      await route.fulfill({ response }).catch(() => {});
      finished();
    });
    return { release, waiting, done };
  }
  let g = await gate();
  await rows(page).first().locator('.match-card').click();
  await g.waiting;
  await rows(page).nth(1).locator('.match-card').click();
  await expect(page.locator('.analysis-heading')).toHaveAttribute(
    'data-match-id',
    f.entries[1].match.id,
  );
  g.release();
  await g.done;
  await expect(page.locator('.analysis-heading')).toHaveAttribute(
    'data-match-id',
    f.entries[1].match.id,
  );
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(rows(page)).toHaveCount(2);
  await page.unroute('**/api/positions?**');
  g = await gate();
  await rows(page).first().locator('.match-card').click();
  await g.waiting;
  await page
    .getByRole('button', { name: 'Clear position filter', exact: true })
    .click();
  await expect(rows(page)).toHaveCount(3);
  g.release();
  await g.done;
  await expect(
    page.getByRole('tab', { name: 'Match database', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(rows(page)).toHaveCount(3);
});

test('current match rows still open while the same database refreshes in the background', async ({
  page,
}) => {
  const f = await fixture(page);
  await expect(rows(page)).toHaveCount(3);
  const snapshot = await (
    await page.request.get(`/api/library/matches?database_id=${f.database.id}`)
  ).json();
  await rows(page).first().locator('.match-card').click();
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  for (const mode of ['button', 'enter', 'double']) {
    let release = () => {},
      started = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const pending = new Promise<void>((resolve) => {
      started = resolve;
    });
    await page.route('**/api/library/matches?**', async (route) => {
      started();
      await held;
      await route.fulfill({ json: snapshot }).catch(() => {});
    });
    await page
      .getByRole('tab', { name: 'Match database', exact: true })
      .click();
    await pending;
    const first = rows(page).first();
    await expect(first.locator('.match-card')).toBeEnabled();
    if (mode === 'button') await first.locator('.match-card').click();
    else if (mode === 'enter') {
      await first.focus();
      await page.keyboard.press('Enter');
    } else await first.locator('td').nth(7).dblclick();
    await expect(page.locator('.analysis-workspace .board')).toBeVisible();
    release();
    await page.unroute('**/api/library/matches?**');
  }
});

test('position-filtered matches paginate in the selected database and show an empty common list', async ({
  page,
}) => {
  const f = await fixture(page, 51);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(50);
  await page
    .getByRole('button', { name: 'Load more matches', exact: true })
    .click();
  await expect(rows(page)).toHaveCount(51);
  const ids = await rows(page).locator('.library-id-cell').allTextContents();
  expect(new Set(ids).size).toBe(51);
  await page
    .getByLabel('Search matches', { exact: true })
    .fill('No such player needle');
  await expect(rows(page)).toHaveCount(0);
  await expect(page.locator('.library-empty')).toBeVisible();
  await expect(page.locator('.position-results')).toHaveCount(0);
});

test('a failed exact occurrence lookup is visible with the search form closed and can be retried', async ({
  page,
}) => {
  const f = await fixture(page);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(2);
  await page.locator('.position-search summary').click();
  await page.route('**/api/positions?**', (route) =>
    new URL(route.request().url()).searchParams.has('position_id')
      ? route.fulfill({
          status: 503,
          json: { error: 'Temporary position lookup failure' },
        })
      : route.continue(),
  );
  await rows(page).first().locator('.match-card').click();
  await expect(
    page.locator('.match-database .error[role=alert]'),
  ).toContainText('Temporary position lookup failure');
  await expect(
    page.getByRole('tab', { name: 'Match database', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await page.unroute('**/api/positions?**');
  await rows(page).first().locator('.match-card').click();
  await expect(page.locator('.position-code code')).toHaveText(f.needle);
  await expect(page.locator('.board-heading')).toContainText('Move 2');
});
