import { fulfillJson, mockEngineCatalog } from './api-fixtures';
import { expect, test, type Page } from './test';

const databaseId = '00000000-0000-0000-0000-000000000001';
const entries = Array.from({ length: 50 }, (_, index) => ({
  match_no: index + 1,
  id: `00000000-0000-0000-0001-${String(index).padStart(12, '0')}`,
  database_id: databaseId,
  filename: `match-${index}.xg`,
  display_name:
    index === 1
      ? 'Training match with a long custom name'
      : `match-${index}.xg`,
  format: 'xg',
  white:
    index === 1
      ? 'A very long player name which still needs to be accessible'
      : `Alice ${index}`,
  black: index === 1 ? 'Longer Opponent' : `Bob ${index}`,
  match_length: 11,
  position_count: 460,
  game_count: 10,
  created_at: '2026-11-09T12:00:00Z',
  played_on: '2026-11-08',
  tournament:
    index === 1
      ? 'An international tournament with a very long title'
      : 'Test Open',
  round: 'Final',
  place: null,
  result_state: 'completed',
  white_score: 11,
  black_score: 9,
  white_pr: 3.25,
  black_pr: 4.15,
  pr_method: 'gnubg_emg_500_v1',
  pr_profile: 'GNUbg 3-ply',
  analysis_status: index === 1 ? 'imported' : 'completed',
  import_warning_count: 0,
  play_assisted: false,
  play_paused: false,
  source_available: index !== 3,
  tags:
    index === 1
      ? Array.from({ length: 10 }, (_, i) => `Long tag ${i}`)
      : ['Study'],
  comment:
    index === 1
      ? 'First line of a long note\nSecond line\n'.repeat(12)
      : 'Review cube decisions',
  version: 0,
}));

async function mockLibrary(page: Page) {
  await page.addInitScript((id) => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.database', id);
  }, databaseId);
  await page.route('**/api/databases', (route) =>
    route.fulfill({
      json: [
        {
          id: databaseId,
          name: 'Compact demo',
          description: '',
          is_default: true,
          is_system: true,
          version: 0,
          match_count: entries.length,
          created_at: entries[0].created_at,
          updated_at: entries[0].created_at,
        },
      ],
    }),
  );
  await page.route('**/api/library/matches?*', (route) =>
    fulfillJson<'LibraryPage'>(route, { items: entries, next_cursor: null }),
  );
  const previewRequests: string[] = [];
  await page.route(
    /\/api\/matches\/[^/]+\/(games|imported-analysis)/,
    (route) => {
      previewRequests.push(route.request().url());
      return route.fulfill({ json: [] });
    },
  );
  return previewRequests;
}

async function library(page: Page) {
  const previewRequests = await mockLibrary(page);
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(
    entries.length,
  );
  return previewRequests;
}

test('loads match pages only while the database workspace is open', async ({
  page,
}) => {
  await mockLibrary(page);
  const requests: URLSearchParams[] = [];
  await page.route('**/api/library/matches?*', (route) => {
    const params = new URL(route.request().url()).searchParams;
    requests.push(params);
    return fulfillJson<'LibraryPage'>(route, {
      items: params.get('page') === '2' ? entries.slice(1, 2) : entries,
      total: 51,
      next_cursor: null,
    });
  });
  await page.clock.install();
  await page.goto('/');
  const tab = page.getByRole('tab', { name: 'Match database', exact: true });
  await expect(tab).toHaveAttribute('aria-selected', 'false');
  await page.clock.runFor(500);
  expect(requests).toHaveLength(0);

  await tab.click();
  const panel = page.getByRole('tabpanel', { name: 'Match database' });
  await expect(panel.locator('tbody tr')).toHaveCount(50);
  expect(
    requests.map((params) => [params.get('page'), params.get('limit')]),
  ).toEqual([['1', '50']]);
  await panel
    .locator('.search-pagination')
    .getByRole('button', { name: 'Page 2', exact: true })
    .click();
  await expect(panel.locator('tbody tr')).toHaveCount(1);
  expect(requests.map((params) => params.get('page'))).toEqual(['1', '2']);
});

test('tracks selected matches after closing the dialog and reloading the page', async ({
  page,
}) => {
  const submissions: Array<{ id: string; body: unknown }> = [];
  const jobs = new Map<string, 'queued' | 'completed' | 'failed'>();
  const job = (id: string) => ({
    id,
    kind: 'match',
    match_uuid: id,
    status: jobs.get(id),
  });
  await mockEngineCatalog(page, () => [
    {
      id: 'gnubg',
      max_parallel_threads: 7,
      name: 'GNUbg',
      available: true,
      version: '1',
      plies: [1, 3],
    },
    {
      id: 'bgsage',
      max_parallel_threads: 9,
      name: 'NeonGammon',
      available: true,
      version: '1',
      plies: [2, 4],
    },
  ]);
  await page.route(/\/api\/matches\/[^/]+\/analysis$/, async (route) => {
    const id = new URL(route.request().url()).pathname.split('/')[3];
    submissions.push({ id, body: route.request().postDataJSON() });
    if (submissions.length === 1) {
      await route.fulfill({
        status: 422,
        json: { error: 'Engine temporarily unavailable' },
      });
      return;
    }
    jobs.set(id, 'queued');
    await route.fulfill({ status: 202, json: job(id) });
  });
  await page.route(/\/api\/jobs\/[^/]+$/, (route) => {
    const id = new URL(route.request().url()).pathname.split('/')[3];
    if (id === 'status' && route.request().method() === 'POST') {
      // Background job reads batch several watched jobs into one request.
      const { ids } = route.request().postDataJSON() as { ids: string[] };
      const known = ids.filter((requested) => jobs.has(requested));
      if (known.length === 0) return route.fallback();
      return route.fulfill({
        json: {
          jobs: known.map(job),
          missing: ids.filter((requested) => !jobs.has(requested)),
          restricted: [],
        },
      });
    }
    return jobs.has(id) ? route.fulfill({ json: job(id) }) : route.fallback();
  });
  await library(page);
  const rows = page.locator('.database-table tbody tr');
  const batch = page.locator('.library-batch-bar');
  const open = batch.getByRole('button', {
    name: 'Analyze selected matches …',
  });
  await rows.nth(3).getByRole('checkbox').check();
  await expect(open).toBeDisabled();
  await rows.nth(0).getByRole('checkbox').check();
  await rows.nth(1).getByRole('checkbox').check();
  for (const name of ['Open match', 'Resume match', 'Edit match details']) {
    await expect(batch.getByRole('button', { name, exact: true })).toBeDisabled();
  }
  await expect(rows.locator('[data-action-icon]')).toHaveCount(0);

  await open.click();
  const dialog = page.getByRole('dialog', { name: 'Analyze selected matches' });
  await expect(dialog).toContainText('Matches unavailable for analysis (1)');
  await page.keyboard.press('Escape');
  await expect(open).toBeFocused();
  expect(submissions).toHaveLength(0);

  await open.click();
  await dialog
    .getByLabel('Analysis engine', { exact: true })
    .selectOption('bgsage');
  await dialog
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('4');
  await dialog
    .getByRole('button', { name: 'Analyze selected matches', exact: true })
    .click();
  await expect(dialog.getByRole('status')).toHaveText(
    'Analysis jobs accepted (1)',
  );
  await expect(dialog.getByRole('alert')).toContainText(
    entries[0].display_name,
  );
  await expect(dialog.getByRole('alert')).toContainText(
    'Engine temporarily unavailable',
  );
  await dialog.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(dialog.getByRole('status')).toHaveText(
    'Analysis jobs accepted (2)',
  );
  await expect(dialog.getByRole('alert')).toHaveCount(0);
  expect(submissions).toEqual(
    [entries[0].id, entries[1].id, entries[0].id].map((id) => ({
      id,
      body: { engine_id: 'bgsage', plies: 4 },
    })),
  );
  await page.keyboard.press('Escape');
  await expect(open).toBeFocused();
  const database = page.getByRole('tabpanel', { name: 'Match database' });
  const progress = database
    .getByRole('status')
    .filter({ hasText: /matches analyzed/ });
  await batch.getByRole('button', { name: 'Clear selection' }).click();
  await expect(progress).toHaveText('Bulk analysis · 0 of 2 matches analyzed');
  jobs.set(entries[0].id, 'completed');
  await expect(progress).toHaveText('Bulk analysis · 1 of 2 matches analyzed');
  await page.reload();
  await expect(database).toBeVisible();
  await expect(progress).toHaveText('Bulk analysis · 1 of 2 matches analyzed');
  expect(submissions).toHaveLength(3);
  jobs.set(entries[1].id, 'failed');
  await expect(progress).toHaveText(
    'Bulk analysis · 1 of 2 matches analyzed · Failed (1)',
  );
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(progress).toBeInViewport();
    await page.screenshot({
      path: test.info().outputPath(`bulk-progress-${width}.png`),
    });
  }
});

for (const [width, height] of [
  [1800, 1169],
  [2560, 1440],
  [3840, 2160],
]) {
  test(`compact full-width library keeps match actions and notes at ${width}×${height}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height });
    const requests = await library(page);
    const rows = page.locator('.database-table tbody tr');
    const first = rows.first(),
      long = rows.nth(1);
    // Measure only once both rows are rendered.
    await expect(long).toBeVisible();
    for (const row of [first, long])
      expect((await row.boundingBox())!.height).toBeLessThanOrEqual(56);
    const sizes = await page.locator('.library-content').evaluate((element) => {
      const table = element.querySelector('table')!;
      return {
        available: element.getBoundingClientRect().width,
        table: table.getBoundingClientRect().width,
      };
    });
    expect(Math.abs(sizes.available - sizes.table)).toBeLessThanOrEqual(3);
    expect(
      (await page.locator('.match-database').boundingBox())!.width,
    ).toBeGreaterThanOrEqual(width - 64);
    expect(
      (await long.locator('.library-notes-cell').boundingBox())!.width,
    ).toBeGreaterThanOrEqual(120);
    await expect(page.locator('.match-preview')).toHaveCount(0);
    await expect(first.locator('.library-id-cell')).toHaveText('1');
    await expect(first).toContainText('Alice 0');
    await expect(first).toContainText('Bob 0');
    await expect(first).not.toContainText('XG');
    await expect(first).toContainText('460');
    await expect(long).toContainText(entries[1].display_name);
    await expect(long.locator('.library-notes-cell')).toContainText(
      'Long tag 9',
    );
    await expect(long.locator('[title]')).toHaveCount(0);
    // The row summary is the row's accessible description; hovering or
    // focusing a row shows no overlay above the rows below it.
    await long.focus();
    await expect(page.getByRole('tooltip')).toHaveCount(0);
    const summary = (await long.getAttribute('aria-description')) ?? '';
    expect(summary).toContain('ID: 2');
    expect(summary).toContain(entries[1].white);
    expect(summary).toContain(entries[1].display_name);
    expect(summary).toContain('Result: 11–9');
    expect(summary).toContain('Positions: 460');
    expect(summary).toContain('Analysis: Imported');
    expect(summary).toContain('Tags: Long tag 0');
    expect(summary).toContain('Comment: First line of a long note');
    await expect(first).toHaveAttribute('aria-description', /ID: 1/);
    expect(await first.getAttribute('aria-description')).not.toBe(summary);
    await first.locator('td').nth(3).click();
    await expect(first).toBeFocused();
    await page.keyboard.press('ArrowDown');
    await expect(long).toBeFocused();
    await expect(
      page.getByRole('tab', { name: 'Match database', exact: true }),
    ).toHaveAttribute('aria-selected', 'true');
    expect(requests).toEqual([]);
    await long.getByRole('checkbox').check();
    const batch = page.locator('.library-batch-bar');
    const edit = batch.getByRole('button', {
      name: 'Edit match details',
      exact: true,
    });
    await expect(edit).toBeVisible();
    await edit.click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByLabel('Name', { exact: true })).toHaveValue(
      entries[1].display_name,
    );
    await expect(dialog.getByLabel('Event', { exact: true })).toHaveValue(
      entries[1].tournament,
    );
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    await first.getByRole('checkbox').check();
    await long.getByRole('checkbox').check();
    await expect(page.locator('.library-batch-bar')).toContainText('2');
    for (const name of [
      'Analyze selected matches …',
      'Copy',
      'Move',
      'Tags',
      'Export',
      'Delete',
    ])
      await expect(
        page
          .locator('.library-batch-bar')
          .getByRole('button', { name, exact: true }),
      ).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: test.info().outputPath(`match-selection-${width}.png`),
    });
    if (width === 1800) {
      await page.setViewportSize({ width: 1440, height: 1100 });
      expect(
        (await long.locator('.library-notes-cell').boundingBox())!.width,
      ).toBeGreaterThanOrEqual(64);
      expect((await long.boundingBox())!.height).toBeLessThanOrEqual(56);
    }
  });
}

test('keeps selection actions visible with wide saved columns', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await library(page);
  const batch = page.locator('.library-batch-bar');
  const actions = batch.getByRole('button');
  await expect(actions).toHaveCount(10);
  for (const action of await actions.all()) {
    await expect(action).toBeInViewport();
    await expect(action).toBeDisabled();
  }
  const columns = page.locator('.library-column-picker');
  await columns.locator('summary').click();
  await columns.getByRole('button', { name: 'Show all columns' }).click();
  await page.keyboard.press('Escape');
  const table = page.locator('.database-table');
  const white = table.locator('th[data-column="white"]');
  await white.getByRole('separator').press('End');
  const wrapper = page.locator('.database-table-wrap');
  await wrapper.evaluate((element) => (element.scrollLeft = 0));
  const row = table.locator('tbody tr').first();
  await row.getByRole('checkbox').check();
  for (const action of [
    batch.getByRole('button', { name: 'Open match', exact: true }),
    batch.getByRole('button', { name: 'Edit match details', exact: true }),
    batch.getByRole('button', { name: 'Export', exact: true }),
  ]) {
    await expect(action).toBeInViewport();
    await expect(action).toBeEnabled();
  }
  await batch
    .getByRole('button', { name: 'Edit match details', exact: true })
    .click();
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await table.locator('tbody tr').nth(1).getByRole('checkbox').check();
  for (const name of ['Open match', 'Resume match', 'Edit match details'])
    await expect(batch.getByRole('button', { name, exact: true })).toBeDisabled();
  for (const name of ['Copy', 'Move', 'Tags', 'Export', 'Delete'])
    await expect(batch.getByRole('button', { name, exact: true })).toBeEnabled();
  await batch.getByRole('button', { name: 'Clear selection', exact: true }).click();
  for (const action of await actions.all()) await expect(action).toBeDisabled();
});

test('uses desktop table space for both player names', async ({ page }) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await library(page);
  for (const column of ['white', 'black']) {
    const header = page.locator(`.database-table th[data-column="${column}"]`);
    expect((await header.boundingBox())!.width).toBeGreaterThanOrEqual(155);
  }
  expect(
    (await page.locator('.database-table tbody tr').first().boundingBox())!
      .height,
  ).toBeLessThanOrEqual(44);
});

test('keeps player names readable in narrow saved columns', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await library(page);
  const black = page.locator('.database-table th[data-column="black"]');
  const handle = black.getByRole('separator');
  await handle.press('Home');
  for (let step = 0; step < 4; step++) await handle.press('ArrowRight');
  expect((await black.boundingBox())!.width).toBeCloseTo(100, 0);
  const player = page.getByText('Longer Opponent', { exact: true });
  const sizes = await player.evaluate((element) => ({
    height: element.clientHeight,
    contentHeight: element.scrollHeight,
    lineHeight: Number.parseFloat(getComputedStyle(element).lineHeight),
  }));
  expect(sizes.height).toBeGreaterThan(sizes.lineHeight);
  expect(sizes.contentHeight).toBe(sizes.height);
});

test('removes one applied filter by keyboard and keeps the other filters', async ({
  page,
}) => {
  await library(page);
  const search = page.getByLabel('Search matches', { exact: true });
  const filters = page.getByRole('button', { name: 'Filters', exact: true });
  const fields = page.locator('.library-filters');
  const eventField = fields.getByLabel('Event', { exact: true });
  const resultField = fields.getByLabel('Result', { exact: true });
  await search.fill('Alice');
  await filters.click();
  await eventField.fill('Test Open');
  await resultField.selectOption('white_won');
  await filters.click();
  const chips = page.getByRole('list', { name: 'Applied filters' });
  const event = chips.getByRole('button', {
    name: 'Remove filter: Event: Test Open',
  });
  await expect(event).toBeVisible();
  await expect(chips).toContainText('White won');
  const request = page.waitForRequest((req) => {
    const url = new URL(req.url());
    return (
      url.pathname === '/api/library/matches' &&
      url.searchParams.get('q') === 'Alice' &&
      url.searchParams.get('result') === 'white_won' &&
      !url.searchParams.get('tournament')
    );
  });
  await event.press('Enter');
  await request;
  await expect(event).toHaveCount(0);
  await expect(filters).toBeFocused();
  await expect(chips).toContainText('Alice');
  await expect(chips).toContainText('White won');
  await filters.click();
  await expect(eventField).toHaveValue('');
  await expect(resultField).toHaveValue('white_won');
});

test('opens search panels from one toolbar and keeps the notes draft', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await library(page);
  const toolbar = page.locator('.library-search-toolbar');
  const filters = toolbar.getByRole('button', { name: 'Filters', exact: true });
  const notes = toolbar.getByRole('button', { name: 'Notes', exact: true });
  const position = toolbar.getByRole('button', {
    name: 'Position ID',
    exact: true,
  });
  await expect(
    toolbar.getByRole('textbox', { name: 'Search matches' }),
  ).toBeVisible();
  await filters.click();
  await expect(filters).toHaveAttribute('aria-expanded', 'true');
  await notes.click();
  await expect(filters).toHaveAttribute('aria-expanded', 'false');
  const text = page
    .locator('.annotation-search')
    .getByRole('searchbox', { name: 'Text' });
  await text.fill('Remember this note search');
  await position.click();
  await expect(notes).toHaveAttribute('aria-expanded', 'false');
  await expect(text).toBeHidden();
  await expect(
    page.locator('.position-search').getByRole('textbox'),
  ).toBeVisible();
  await notes.click();
  await expect(text).toHaveValue('Remember this note search');
});
