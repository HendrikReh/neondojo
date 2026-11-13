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
  pr_profile: 'GNUbg 2-ply',
  analysis_status: index === 1 ? 'imported' : 'completed',
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

async function library(page: Page) {
  await page.addInitScript((id) => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.database', id);
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
    route.fulfill({ json: { items: entries, next_cursor: null } }),
  );
  const previewRequests: string[] = [];
  await page.route(
    /\/api\/matches\/[^/]+\/(games|imported-analysis)/,
    (route) => {
      previewRequests.push(route.request().url());
      return route.fulfill({ json: [] });
    },
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(
    entries.length,
  );
  return previewRequests;
}

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
    await long.focus();
    const tooltip = page.getByRole('tooltip');
    await expect(tooltip).toBeVisible();
    const summary = await tooltip.innerText();
    expect(summary).toContain('ID: 2');
    expect(summary).toContain(entries[1].white);
    expect(summary).toContain(entries[1].display_name);
    expect(summary).toContain('Result: 11–9');
    expect(summary).toContain('Positions: 460');
    expect(summary).toContain('Analysis: Imported');
    expect(summary).toContain('Tags: Long tag 0');
    expect(summary).toContain('Comment: First line of a long note');
    await first.focus();
    await expect(tooltip).toContainText('ID: 1');
    await expect(tooltip).not.toHaveText(summary);
    await first.locator('td').nth(3).click();
    await expect(first).toBeFocused();
    await page.keyboard.press('ArrowDown');
    await expect(long).toBeFocused();
    await expect(
      page.getByRole('tab', { name: 'Match database', exact: true }),
    ).toHaveAttribute('aria-selected', 'true');
    expect(requests).toEqual([]);
    await expect(
      first.getByRole('link', { name: 'Export: SGF' }),
    ).toHaveAttribute('href', `/api/matches/${entries[0].id}/source`);
    const edit = long.getByRole('button', {
      name: `Edit match details: ${entries[1].display_name}`,
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
    for (const name of ['Copy', 'Move', 'Tags', 'Export', 'Delete'])
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
    if (width === 1800) {
      await page.setViewportSize({ width: 1440, height: 1100 });
      expect(
        (await long.locator('.library-notes-cell').boundingBox())!.width,
      ).toBeGreaterThanOrEqual(64);
      expect((await long.boundingBox())!.height).toBeLessThanOrEqual(56);
    }
  });
}

test('keeps row actions visible with wide saved columns', async ({ page }) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await library(page);
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
  const viewport = (await wrapper.boundingBox())!;
  for (const action of [
    row.getByRole('button', { name: /Open match:/ }),
    row.getByRole('button', { name: /Edit match details:/ }),
    row.getByRole('link', { name: 'Export: SGF' }),
  ]) {
    await expect(action).toBeInViewport();
    const box = (await action.boundingBox())!;
    expect(box.x + box.width).toBeLessThanOrEqual(viewport.x + viewport.width);
  }
  await row.getByRole('button', { name: /Edit match details:/ }).click();
  await expect(page.getByRole('dialog')).toBeVisible();
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
