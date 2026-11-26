import type { components } from '../src/generated-api';
import { fulfillJson } from './api-fixtures';
import { expect, test, type Page } from './test';

test.use({ actionTimeout: 10000 });
async function fixture(page: Page, count = 2, repeatWithinGame = false) {
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
    const reset =
      ';PL[W]AE[a:y]AW[x][x][m][m][m][m][m][h][h][h][f][f][f][f][f]AB[a][a][l][l][l][l][l][q][q][q][s][s][s][s][s];W[31hefe];B[42qusu]';
    const repeatedGame = repeatWithinGame
      ? game.slice(0, -1) + reset + ')'
      : game;
    const response = await page.request.post(
      `/api/imports?database_id=${database.id}`,
      {
        multipart: {
          file: {
            name: `positions-${n}.sgf`,
            mimeType: 'application/octet-stream',
            buffer: Buffer.from(n === 0 ? repeatedGame + game : game),
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
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.database', id);
  }, database.id);
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  return { database, entries, needle: entries[0].positions[1].position_id };
}
async function search(page: Page, id: string) {
  const form = page.locator('.position-search');
  if (!(await form.isVisible()))
    await page
      .getByRole('button', { name: 'Position ID', exact: true })
      .click();
  await form.getByRole('textbox').fill(id);
  await form
    .getByRole('button', { name: 'Find exact match', exact: true })
    .click();
}
const rows = (page: Page) => page.locator('.database-table tbody tr');

test('retrying a failed match page clears the error and keeps the loaded rows', async ({
  page,
}) => {
  const { database } = await fixture(page);
  const first: components['schemas']['LibraryPage'] = await (
    await page.request.get(`/api/library/matches?database_id=${database.id}`)
  ).json();
  let attempts = 0;
  await page.route('**/api/library/matches?*', (route) => {
    const params = new URL(route.request().url()).searchParams;
    if (params.get('database_id') !== database.id) return route.fallback();
    if (params.get('page') !== '2')
      return fulfillJson<'LibraryPage'>(route, {
        items: first.items.slice(0, 1),
        total: 52,
        next_cursor: 'second',
      });
    if (++attempts === 1)
      return fulfillJson<'ErrorBody'>(
        route,
        { error: 'Page unavailable' },
        503,
      );
    return fulfillJson<'LibraryPage'>(route, {
      items: first.items.slice(1),
      total: 52,
      next_cursor: null,
    });
  });
  await page.reload();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(rows(page)).toHaveCount(1);
  const more = page.getByRole('button', {
    name: 'Next page',
    exact: true,
  });
  const error = page
    .locator('.library-search-toolbar')
    .locator('..')
    .getByRole('alert');
  await more.click();
  await expect(error).toBeVisible();
  await expect(rows(page)).toHaveCount(1);
  await more.click();
  await expect(rows(page)).toHaveCount(2);
  await expect(error).toHaveCount(0);
});

test('position matches use the common table, preserve other filters and open the exact occurrence', async ({
  page,
}) => {
  const f = await fixture(page);
  await expect(rows(page)).toHaveCount(3);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(3);
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
  await expect(rows(page)).toHaveCount(2);
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
  await expect(rows(page)).toHaveCount(3);
  for (const mode of ['button', 'enter', 'double']) {
    const first = rows(page).first();
    if (mode === 'button') await first.locator('.match-card').click();
    else if (mode === 'enter') {
      await first.focus();
      await page.keyboard.press('Enter');
    } else await first.locator('.library-id-cell').dblclick();
    await expect(page.locator('.analysis-heading')).toHaveAttribute(
      'data-match-id',
      f.entries[0].match.id,
    );
    await expect(page.locator('.board-heading')).toContainText('Move 2');
    await expect(page.locator('.position-code code')).toHaveText(f.needle);
    await page
      .getByRole('tab', { name: 'Match database', exact: true })
      .click();
    await expect(rows(page)).toHaveCount(3);
  }
});

test('late position lookups cannot override a newer row click or changed table filters', async ({
  page,
}) => {
  const f = await fixture(page);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(3);
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
    await page.route('**/api/positions/*', async (route) => {
      const id = new URL(route.request().url()).pathname.split('/').at(-1);
      if (id !== f.entries[0].positions[1].id) return route.continue();
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
  await rows(page).nth(2).locator('.match-card').click();
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
  await expect(rows(page)).toHaveCount(3);
  await page.unroute('**/api/positions/*');
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
  const snapshot: components['schemas']['LibraryPage'] = await (
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
      await fulfillJson<'LibraryPage'>(route, snapshot).catch(() => {});
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
    } else await first.locator('.library-id-cell').dblclick();
    await expect(page.locator('.analysis-workspace .board')).toBeVisible();
    release();
    await page.unroute('**/api/library/matches?**');
  }
});

test('position-filtered matches paginate in the selected database and show an empty common list', async ({
  page,
}) => {
  const f = await fixture(page, 51, true);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(50);
  const moveHeading = page.locator('th[data-column="occurrence_move"]');
  await moveHeading.locator('button').click();
  await moveHeading.locator('button').click();
  await expect(moveHeading).toHaveAttribute('aria-sort', 'descending');
  await expect(
    rows(page).first().locator('[data-column="occurrence_move"]'),
  ).toHaveText('4');
  await expect(rows(page)).toHaveCount(50);
  const firstPageMoves = await rows(page)
    .locator('[data-column="occurrence_move"]')
    .allTextContents();
  const firstPageIds = await rows(page).evaluateAll((nodes) =>
    nodes.map((n) => (n as HTMLElement).dataset.positionId),
  );
  const firstPageMatchIds = await rows(page)
    .locator('.library-id-cell')
    .allTextContents();
  await page.getByRole('button', { name: 'Last', exact: true }).click();
  await expect(rows(page)).toHaveCount(3);
  const moves = [
    ...firstPageMoves,
    ...(await rows(page)
      .locator('[data-column="occurrence_move"]')
      .allTextContents()),
  ].map(Number);
  expect(moves).toEqual([...moves].sort((a, b) => b - a));
  const ids = await rows(page).evaluateAll((nodes) =>
    nodes.map((n) => (n as HTMLElement).dataset.positionId),
  );
  expect(new Set([...firstPageIds, ...ids]).size).toBe(53);
  expect(
    new Set([
      ...firstPageMatchIds,
      ...(await rows(page).locator('.library-id-cell').allTextContents()),
    ]).size,
  ).toBe(51);
  await page
    .getByLabel('Search matches', { exact: true })
    .fill('No such player needle');
  await expect(rows(page)).toHaveCount(0);
  await expect(page.locator('.library-empty')).toBeVisible();
  const empty = page.locator('.library-empty');
  await expect(
    empty.getByRole('button', { name: 'Search all databases' }),
  ).toBeVisible();
  await empty.getByRole('button', { name: 'Clear filters' }).click();
  await expect(page.getByLabel('Search matches', { exact: true })).toHaveValue(
    '',
  );
  await expect(rows(page)).toHaveCount(50);
  await expect(page.locator('.position-results')).toHaveCount(0);
});

test('a failed exact occurrence lookup is visible with the search form closed and can be retried', async ({
  page,
}) => {
  const f = await fixture(page);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(3);
  await page.getByRole('button', { name: 'Position ID', exact: true }).click();
  await page.route('**/api/positions/*', (route) =>
    route.fulfill({
      status: 503,
      json: { error: 'Temporary position lookup failure' },
    }),
  );
  await rows(page).first().locator('.match-card').click();
  await expect(
    page.locator('.match-database .error[role=alert]'),
  ).toContainText('Temporary position lookup failure');
  await expect(
    page.getByRole('tab', { name: 'Match database', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await page.unroute('**/api/positions/*');
  await rows(page).first().locator('.match-card').click();
  await expect(page.locator('.position-code code')).toHaveText(f.needle);
  await expect(page.locator('.board-heading')).toContainText('Move 2');
});

test('each repeated occurrence has database, game and move and opens its own location', async ({
  page,
}) => {
  const f = await fixture(page, 2, true);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(4);
  await expect(
    rows(page).locator('[data-column="occurrence_database"]'),
  ).toHaveText(Array(4).fill(f.database.name));
  const repeated = page.locator(`tr[data-match-id="${f.entries[0].match.id}"]`);
  await expect(repeated).toHaveCount(3);
  await repeated.first().getByRole('checkbox').check();
  await expect(page.locator('.library-batch-bar')).toContainText(
    '1 match selected',
  );
  await expect(repeated.first().getByRole('checkbox')).toHaveAccessibleName(
    `Select match ${f.entries[0].metadata.display_name}`,
  );
  await expect(
    page.getByRole('checkbox', { name: 'Select all loaded matches' }),
  ).toHaveJSProperty('indeterminate', true);
  await expect(repeated.nth(1).getByRole('checkbox')).toBeChecked();
  await expect(repeated.nth(2).getByRole('checkbox')).toBeChecked();
  const secondInGame = repeated
    .filter({
      has: page.locator('[data-column="occurrence_game"]', { hasText: /^1$/ }),
    })
    .filter({
      has: page.locator('[data-column="occurrence_move"]', { hasText: /^4$/ }),
    });
  await secondInGame
    .getByRole('button', { name: 'Open position in game 1, move 4' })
    .click();
  await expect(page.locator('.board-heading')).toContainText('Move 4');
  await expect(page.locator('.game-selector select')).toHaveValue('1');
  await expect(page.locator('.position-code code')).toHaveText(f.needle);
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(
    rows(page).filter({
      has: page.locator('[data-column="occurrence_move"]', { hasText: /^4$/ }),
    }),
  ).toHaveClass('selected');
  const secondGame = repeated.filter({
    has: page.locator('[data-column="occurrence_game"]', { hasText: /^2$/ }),
  });
  await expect(
    secondGame.locator('[data-column="occurrence_move"]'),
  ).toHaveText('2');
  await secondGame.locator('.match-card').click();
  await expect(page.locator('.board-heading')).toContainText('Move 2');
  await expect(page.locator('.game-selector select')).toHaveValue('2');
});

test('database, game and move headings sort occurrences in both directions', async ({
  page,
}) => {
  const f = await fixture(page, 2, true);
  const other = await (
    await page.request.post('/api/databases', {
      data: { name: `alpha ${crypto.randomUUID()}` },
    })
  ).json();
  const imported = await page.request.post(
    `/api/imports?database_id=${other.id}`,
    {
      multipart: {
        file: {
          name: 'other-occurrence.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            `(;GM[6]PW[${f.entries[0].match.white}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
          ),
        },
      },
    },
  );
  expect(imported.status()).toBe(201);
  await page.reload();
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page
    .locator('.database-sidebar')
    .getByRole('button', { name: /All databases/ })
    .click();
  await page
    .getByLabel('Search matches', { exact: true })
    .fill(f.entries[0].match.white);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(4);
  for (const key of [
    'occurrence_database',
    'occurrence_game',
    'occurrence_move',
  ]) {
    const heading = page.locator(`th[data-column="${key}"]`);
    const button = heading.locator('button');
    for (const direction of ['ascending', 'descending']) {
      const completed = page.waitForResponse((response) => {
        const url = new URL(response.url());
        return (
          url.pathname === '/api/library/matches' &&
          url.searchParams.get('sort') === key &&
          url.searchParams.get('direction') ===
            (direction === 'ascending' ? 'asc' : 'desc')
        );
      });
      await button.click();
      await completed;
      await expect(heading).toHaveAttribute('aria-sort', direction);
      await expect(rows(page)).toHaveCount(4);
      await expect
        .poll(async () => {
          const values = await rows(page)
            .locator(`[data-column="${key}"]`)
            .allTextContents();
          const expected = [...values].sort((a, b) =>
            key === 'occurrence_database'
              ? a.toLowerCase().localeCompare(b.toLowerCase())
              : Number(a) - Number(b),
          );
          if (direction === 'descending') expected.reverse();
          return JSON.stringify(values) === JSON.stringify(expected);
        })
        .toBe(true);
    }
  }
  const occurrence = f.entries[0].positions.find(
    (p: { game_no: number; ply: number }) => p.game_no === 1 && p.ply === 4,
  );
  await rows(page).first().locator('.match-card').click();
  await expect(page.locator('.board-heading')).toContainText('Move 4');
  await expect(page.locator('.game-selector select')).toHaveValue('1');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(
    page.locator(`tr[data-position-id="${occurrence.id}"]`),
  ).toHaveClass('selected');
  await page
    .getByRole('button', { name: 'Clear position filter', exact: true })
    .click();
  await expect(rows(page)).toHaveCount(2);
  await expect(page.locator('[data-column="occurrence_move"]')).toHaveCount(0);
  await expect(page.locator('.match-database .error')).toHaveCount(0);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(4);
  await expect(
    page.locator('th[data-column="occurrence_move"]'),
  ).toHaveAttribute('aria-sort', 'descending');
});

test('position-search column widths resize and persist in the browser', async ({
  page,
}) => {
  const f = await fixture(page, 2, true);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(4);
  const header = (id: string) => page.locator(`th[data-column="${id}"]`);
  const width = async (id: string) => (await header(id).boundingBox())!.width;
  const localWidths = () => page.evaluate(() =>
    JSON.parse(localStorage.getItem('neondojo.layout.library.columns.v1') ?? 'null'),
  );
  const saved: Record<string, number> = {};
  for (const [id, delta] of [
    ['occurrence_database', 60],
    ['occurrence_game', 30],
    ['occurrence_move', 20],
  ] as const) {
    const handle = header(id).getByRole('separator');
    await expect(handle).toBeVisible();
    const before = await width(id),
      other = await width('white');
    const rect = (await handle.boundingBox())!;
    await page.mouse.move(rect.x + rect.width / 2, rect.y + rect.height / 2);
    await page.mouse.down();
    await page.mouse.move(
      rect.x + rect.width / 2 + delta,
      rect.y + rect.height / 2,
      { steps: 8 },
    );
    await page.mouse.up();
    await expect
      .poll(() => width(id))
      .toBeCloseTo(Math.round(before) + delta, 0);
    expect(Math.abs((await width('white')) - other)).toBeLessThanOrEqual(1);
    await handle.focus();
    await page.keyboard.press('ArrowRight');
    await expect
      .poll(() => width(id))
      .toBeCloseTo(Math.round(before) + delta + 10, 0);
    saved[id] = Math.round(await width(id));
  }
  await expect(header('created_at')).toHaveAttribute('aria-sort', 'descending');
  await expect
    .poll(async () => {
      const widths = await localWidths();
      return Object.fromEntries(
        Object.keys(saved).map((id) => [
          id,
          widths?.[id],
        ]),
      );
    })
    .toEqual(saved);
  await page.reload();
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(rows(page)).toHaveCount(4);
  for (const [id, value] of Object.entries(saved))
    await expect.poll(() => width(id)).toBeCloseTo(value, 0);
  await page
    .getByRole('button', { name: 'Clear position filter', exact: true })
    .click();
  await expect(rows(page)).toHaveCount(3);
  await search(page, f.needle);
  await expect(rows(page)).toHaveCount(4);
  for (const [id, value] of Object.entries(saved))
    await expect.poll(() => width(id)).toBeCloseTo(value, 0);
  const handle = header('occurrence_database').getByRole('separator');
  const rect = (await handle.boundingBox())!;
  await page.mouse.move(rect.x + rect.width / 2, rect.y + rect.height / 2);
  await page.mouse.down();
  await page.mouse.move(rect.x + rect.width / 2 + 80, rect.y + rect.height / 2);
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expect
    .poll(() => width('occurrence_database'))
    .toBeCloseTo(saved.occurrence_database, 0);
  await page
    .getByRole('button', { name: 'Reset column widths', exact: true })
    .click();
  await expect.poll(localWidths).toBeNull();
  await expect(page.locator('.library-table-resizing')).toHaveCount(0);
});
