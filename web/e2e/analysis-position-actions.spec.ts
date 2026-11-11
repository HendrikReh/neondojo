import { test, expect, type Page } from './test';
async function fixture(
  page: Page,
  locale = 'en',
  failedBookmark = false,
  databaseId?: string,
) {
  const response = await page.request.post(
    `/api/imports${databaseId ? `?database_id=${databaseId}` : ''}`,
    {
      multipart: {
        file: {
          name: 'position-actions.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            `(;GM[6]PW[Toolbar ${crypto.randomUUID()}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])`,
          ),
        },
      },
    },
  );
  expect(response.ok()).toBe(true);
  const imported = await response.json();
  const positions = await (
    await page.request.get(
      `/api/positions?match_id=${imported.match.id}&limit=100`,
    )
  ).json();
  const position = positions.items[0];
  await page.addInitScript(
    ({ id, locale, databaseId }) => {
      localStorage.setItem('bgtutor.locale', locale);
      if (databaseId) localStorage.setItem('bgtutor.database', databaseId);
      localStorage.setItem(
        'bgtutor.workspace',
        JSON.stringify({ matchId: id, gameNo: 1 }),
      );
    },
    { id: imported.match.id, locale, databaseId },
  );
  if (failedBookmark)
    await page.route(`**/api/positions/${position.id}/bookmark`, (r) =>
      r.fulfill({ status: 503, json: { error: 'Unavailable' } }),
    );
  await page.goto('/');
  await expect(page.locator('.analysis-workspace .board')).toBeVisible();
  return position;
}
for (const [width, locale] of [
  [1800, 'en'],
  [390, 'de'],
] as const)
  test(`position actions are adjacent above the board at ${width}px in ${locale}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1100 });
    await fixture(page, locale);
    const toolbar = page.locator('.board-review-toolbar');
    const bookmark = toolbar.getByRole('button', {
      name: locale === 'de' ? 'Position merken' : 'Save position',
      exact: true,
    });
    const search = toolbar.getByRole('button', {
      name: locale === 'de' ? 'Diese Stellung suchen' : 'Search this position',
      exact: true,
    });
    await expect(bookmark).toBeVisible();
    await expect(search).toBeVisible();
    await expect(page.locator('.analysis-heading')).toHaveAttribute(
      'aria-busy',
      'false',
    );
    // The estimate above the toolbar wraps onto multiple lines on mobile.
    await expect(
      page.locator('.analysis-time-estimate small').first(),
    ).toBeVisible();
    await expect(page.locator('.position-code button')).toHaveCount(0);
    // The analysis header may grow as metadata arrives. Read all rectangles
    // in one layout snapshot so a change between reads cannot skew the test.
    const { b, s, board } = await toolbar.evaluate((element) => ({
      b: element
        .querySelector('.position-bookmark-control button')!
        .getBoundingClientRect()
        .toJSON(),
      s: element
        .querySelector('button[data-action-icon="search"]')!
        .getBoundingClientRect()
        .toJSON(),
      board: element
        .closest('.analysis-workspace')!
        .querySelector('.board')!
        .getBoundingClientRect()
        .toJSON(),
    }));
    expect(s.x).toBeGreaterThanOrEqual(b.x + b.width + 4);
    expect(s.x - b.x - b.width).toBeLessThanOrEqual(12);
    expect(s.y).toBeCloseTo(b.y, 0);
    expect(s.width).toBeGreaterThanOrEqual(44);
    expect(s.height).toBeGreaterThanOrEqual(44);
    expect(s.y + s.height).toBeLessThanOrEqual(board.y);
    expect(s.x + s.width).toBeLessThanOrEqual(width);
    await search.focus();
    await expect(page.getByRole('tooltip')).toHaveText(
      locale === 'de' ? 'Diese Stellung suchen' : 'Search this position',
    );
    await page.keyboard.press('Escape');
    await expect(page.getByRole('tooltip')).toHaveCount(0);
    await search.blur();
    await page.mouse.move(1, 1);
    await search.hover();
    await expect(page.getByRole('tooltip')).toHaveText(
      locale === 'de' ? 'Diese Stellung suchen' : 'Search this position',
    );
    await page.screenshot({
      path: test.info().outputPath(`position-actions-${width}.png`),
    });
  });
test('one click opens the database and runs the current source-position search', async ({
  page,
}) => {
  const position = await fixture(page);
  // Search the underlying analysis position even while the board shows Final.
  await page.getByRole('button', { name: 'Final', exact: true }).click();
  const response = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      new URL(r.url()).searchParams.get('position_id') === position.position_id,
  );
  await page
    .locator('.board-review-toolbar')
    .getByRole('button', { name: 'Search this position', exact: true })
    .click();
  await expect(
    page.getByRole('tab', { name: 'Match database', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(
    page.locator('.position-search').getByRole('textbox'),
  ).toBeVisible();
  await expect(
    page.locator('.position-search').getByRole('textbox'),
  ).toHaveValue(position.position_id);
  expect((await response).ok()).toBe(true);
});

test('search remains available when bookmark status cannot be loaded', async ({
  page,
}) => {
  await fixture(page, 'en', true);
  const toolbar = page.locator('.board-review-toolbar');
  await expect(toolbar.getByRole('alert')).toBeVisible();
  const search = toolbar.getByRole('button', {
    name: 'Search this position',
    exact: true,
  });
  await expect(search).toBeEnabled();
  await search.click();
  await expect(
    page.getByRole('tab', { name: 'Match database', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
});

test('explicit searches clear prior scope and filters, rerun the position and do not restore cleared intent', async ({
  page,
}) => {
  const position = await fixture(page);
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  const filtered = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      new URL(r.url()).searchParams.get('q') === 'Toolbar',
  );
  await page.getByLabel('Search matches', { exact: true }).fill('Toolbar');
  expect(new URL((await filtered).url()).searchParams.get('q')).toBe('Toolbar');
  const search = async () => {
    await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
    const response = page.waitForResponse(
      (r) =>
        new URL(r.url()).pathname === '/api/library/matches' &&
        new URL(r.url()).searchParams.get('position_id') ===
          position.position_id,
    );
    await page
      .getByRole('button', { name: 'Search this position', exact: true })
      .click();
    const next = new URL((await response).url()).searchParams;
    expect(next.has('database_id')).toBe(false);
    expect(next.has('q')).toBe(false);
  };
  await search();
  await expect(page.getByLabel('Search matches', { exact: true })).toHaveValue(
    '',
  );
  await page
    .locator('.database-table tbody tr')
    .first()
    .getByRole('checkbox')
    .check();
  await expect(page.locator('.library-batch-bar')).toBeVisible();
  await search();
  await expect(page.locator('.library-batch-bar')).toHaveCount(0);
  const cleared = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      !new URL(r.url()).searchParams.has('position_id'),
  );
  await page
    .getByRole('button', { name: 'Clear position filter', exact: true })
    .click();
  await cleared;
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(page.locator('.library-position-filter')).toHaveCount(0);
  await expect(page.locator('.position-search input')).toHaveValue('');
});

test('a later source-position search cannot be replaced by a delayed earlier result', async ({
  page,
}) => {
  const position = await fixture(page);
  const metadata = await (
    await page.request.get(`/api/matches/${position.match_uuid}/metadata`)
  ).json();
  let release = () => {},
    finished = () => {};
  const delayed = new Promise<void>((resolve) => {
      release = resolve;
    }),
    done = new Promise<void>((resolve) => {
      finished = resolve;
    });
  await page.route('**/api/library/matches?*', async (route) => {
    const query = new URL(route.request().url()).searchParams;
    const source = query.get('position_id');
    if (!source) return route.continue();
    if (source === position.position_id) {
      await delayed;
      await route
        .fulfill({
          json: {
            items: [
              {
                ...metadata,
                white: 'Stale source result',
                display_name: 'Stale source result',
              },
            ],
            next_cursor: null,
          },
        })
        .catch(() => {});
      finished();
    } else
      await route.fulfill({
        json: {
          items: [
            {
              ...metadata,
              white: 'Current source result',
              display_name: 'Current source result',
            },
          ],
          next_cursor: null,
        },
      });
  });
  const oldRequest = page.waitForRequest(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      new URL(r.url()).searchParams.get('position_id') === position.position_id,
  );
  await page
    .getByRole('button', { name: 'Search this position', exact: true })
    .click();
  await oldRequest;
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  await page.getByRole('button', { name: 'Next move', exact: true }).click();
  await expect(page.locator('.position-code code')).not.toHaveText(
    position.position_id,
  );
  const nextId = await page.locator('.position-code code').textContent();
  await page
    .getByRole('button', { name: 'Search this position', exact: true })
    .click();
  await expect(page.locator('.position-search input')).toHaveValue(nextId!);
  await expect(page.locator('.database-table')).toContainText(
    'Current source result',
  );
  release();
  await done;
  await expect(page.locator('.database-table')).not.toContainText(
    'Stale source result',
  );
  await expect(page.locator('.position-search input')).toHaveValue(nextId!);
});

test('analysis search finds occurrences in different databases and clears every prior filter', async ({
  page,
}) => {
  const suffix = crypto.randomUUID();
  const db1 = await (
    await page.request.post('/api/databases', {
      data: { name: `Global source ${suffix}` },
    })
  ).json();
  const db2 = await (
    await page.request.post('/api/databases', {
      data: { name: `Global other ${suffix}` },
    })
  ).json();
  const position = await fixture(page, 'en', false, db1.id);
  const otherName = `Other occurrence ${suffix}`;
  const imported = await page.request.post(
    `/api/imports?database_id=${db2.id}`,
    {
      multipart: {
        file: {
          name: 'global-occurrence.sgf',
          mimeType: 'application/octet-stream',
          buffer: Buffer.from(
            `(;GM[6]PW[${otherName}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe])`,
          ),
        },
      },
    },
  );
  expect(imported.ok()).toBe(true);
  const other = await imported.json();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page.locator('.database-table button[data-sort="id"]').click();
  await page
    .getByLabel('Search matches', { exact: true })
    .fill('exclude-all-occurrences');
  await page.getByRole('button', { name: 'Filters', exact: true }).click();
  const filters = page.locator('.library-filters');
  for (const label of [
    'Player',
    'Match name',
    'Tournament',
    'Round',
    'Tag',
    'Comment',
  ])
    await filters.getByLabel(label, { exact: true }).fill('old-filter');
  const fields = await filters.locator('input[type="date"]').all();
  await fields[0].fill('2030-01-01');
  await fields[1].fill('2040-01-01');
  await filters
    .getByLabel('Date type', { exact: true })
    .selectOption('imported');
  await filters.getByLabel('Match type', { exact: true }).selectOption('money');
  await filters.getByLabel('Result', { exact: true }).selectOption('draw');
  const numeric = await filters.locator('input[type="number"]').all();
  await numeric[0].fill('25');
  await numeric[1].fill('30');
  await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
  const response = page.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/library/matches' &&
      new URL(r.url()).searchParams.get('position_id') === position.position_id,
  );
  await page
    .getByRole('button', { name: 'Search this position', exact: true })
    .click();
  const params = new URL((await response).url()).searchParams;
  expect(params.get('sort')).toBe('id');
  expect(params.get('direction')).toBe('asc');
  expect(
    [...params.keys()].filter(
      (key) =>
        !['position_id', 'sort', 'direction', 'limit', 'date_field'].includes(
          key,
        ),
    ),
  ).toEqual([]);
  await expect(
    page
      .locator('.database-sidebar')
      .getByRole('button', { name: /All databases/ }),
  ).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByLabel('Search matches', { exact: true })).toHaveValue(
    '',
  );
  expect(
    await filters
      .locator('input')
      .evaluateAll((inputs) =>
        inputs
          .map((input) => (input as HTMLInputElement).value)
          .filter(Boolean),
      ),
  ).toEqual([position.position_id]);
  await expect(
    page.locator(`tr[data-match-id="${position.match_uuid}"]`),
  ).toBeVisible();
  await expect(
    page.locator(`tr[data-match-id="${other.match.id}"]`),
  ).toBeVisible();
  await expect(page.locator('.database-table')).toContainText(otherName);
  await expect
    .poll(() =>
      page.evaluate(async () => {
        const state = await (await fetch('/api/configuration')).json();
        return state.current_configuration.library.database_id;
      }),
    )
    .toBe('');
});
