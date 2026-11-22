import { expect, test, type Page } from './test';

// Automatic Match Database widths keep PR values with their import warning,
// the play mode, and both dates readable in every language, and only clipped
// cell text gets a tooltip.
const databaseId = '00000000-0000-0000-0000-00000000f17d';
const base = {
  database_id: databaseId,
  format: 'xg',
  match_length: 11,
  position_count: 460,
  game_count: 10,
  created_at: '2026-09-30T12:00:00Z',
  played_on: '2026-09-28',
  tournament: 'Test Open',
  round: 'Final',
  place: null,
  result_state: 'completed',
  white_score: 11,
  black_score: 9,
  pr_method: 'gnubg_emg_500_v1',
  pr_profile: 'GNUbg 3-ply',
  analysis_status: 'completed',
  source_available: true,
  tags: ['Study'],
  comment: 'Review cube decisions',
  version: 0,
  play_session_id: null,
  play_phase: null,
  play_paused: false,
};
const entries = [
  {
    ...base,
    match_no: 1,
    id: '00000000-0000-0000-0001-00000000f17d',
    filename: 'competition.xg',
    display_name: 'competition.xg',
    white: 'Alice',
    black: 'Bob',
    white_pr: 172.6,
    black_pr: 108.4,
    import_warning_count: 2,
    play_mode: 'competition',
    play_assisted: false,
  },
  {
    ...base,
    match_no: 2,
    id: '00000000-0000-0000-0002-00000000f17d',
    filename: 'coach.xg',
    display_name: 'coach.xg',
    white:
      'A very long player name which still needs to be accessible even when it wraps onto a second line of the table cell',
    black: 'Bob',
    white_pr: null,
    black_pr: null,
    import_warning_count: 0,
    play_mode: 'coach',
    play_assisted: true,
    tournament:
      'An international tournament with a very long title that no automatic column width shows completely in one line',
  },
];
const fitted = [
  'white_pr',
  'black_pr',
  'play_mode',
  'match_length',
  'score',
  'played_at',
  'created_at',
  'position_count',
  'analysis_status',
];

async function open(page: Page, locale: string) {
  await page.addInitScript(
    ([language, id]) => {
      localStorage.setItem('neondojo.locale', language);
      localStorage.setItem('neondojo.database', id);
    },
    [locale, databaseId],
  );
  await page.route('**/api/databases', (route) =>
    route.fulfill({
      json: [
        {
          id: databaseId,
          name: 'Column fit',
          description: '',
          is_default: true,
          is_system: false,
          version: 0,
          match_count: entries.length,
          created_at: base.created_at,
          updated_at: base.created_at,
        },
      ],
    }),
  );
  await page.route('**/api/library/matches?*', (route) =>
    route.fulfill({
      json: { items: entries, total: entries.length, next_cursor: null },
    }),
  );
  await page.goto('/');
  await page.locator('#workspace-tab-database').click();
  await expect(page.locator('.database-table tbody tr')).toHaveCount(
    entries.length,
  );
}

/** Columns whose header label or cell text CSS has clipped. */
function clippedColumns(page: Page) {
  return page.locator('.database-table').evaluate((table, columns) => {
    const clipped = (element: Element) =>
      element.scrollWidth > element.clientWidth + 0.5;
    const result: string[] = [];
    for (const column of columns) {
      const header = table.querySelector(`th[data-column="${column}"]`)!;
      const index = [...header.parentElement!.children].indexOf(header);
      const labels = header.querySelectorAll(
        '.library-sort-label, .action-label',
      );
      if (clipped(header) || [...labels].some(clipped))
        result.push(`${column} header`);
      for (const row of table.querySelectorAll('tbody tr')) {
        const cell = row.children[index];
        if (
          clipped(cell) ||
          [...cell.querySelectorAll('[data-tooltip-overflow]')].some(clipped)
        )
          result.push(`${column} cell`);
      }
    }
    return result;
  }, fitted);
}

for (const [width, height] of [
  [1800, 1169],
  [2560, 1440],
]) {
  for (const locale of ['en', 'de', 'es', 'fr', 'it', 'pl']) {
    test(`automatic library columns fit their text in ${locale} at ${width}×${height}`, async ({
      page,
    }) => {
      await page.setViewportSize({ width, height });
      await open(page, locale);
      expect(await clippedColumns(page)).toEqual([]);
      const sizes = await page
        .locator('.library-content')
        .evaluate((element) => ({
          available: element.getBoundingClientRect().width,
          table: element.querySelector('table')!.getBoundingClientRect().width,
        }));
      expect(Math.abs(sizes.available - sizes.table)).toBeLessThanOrEqual(3);
      if (locale === 'en' || locale === 'de')
        await page.screenshot({
          path: test.info().outputPath(`match-database-${locale}-${width}.png`),
        });
    });
  }
}

test('only clipped library cells show a tooltip, and rows show none', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  await open(page, 'en');
  const rows = page.locator('.database-table tbody tr');
  const tooltip = page.getByRole('tooltip');
  await rows.first().locator('td.library-pr-cell').first().hover();
  await page.waitForTimeout(500);
  await expect(tooltip).toHaveCount(0);
  await rows.first().focus();
  await page.waitForTimeout(500);
  await expect(tooltip).toHaveCount(0);
  await expect(rows.first()).toHaveAttribute(
    'aria-description',
    /Competition · Independent/,
  );
  const event = rows.nth(1).locator('td').filter({
    hasText: 'An international tournament',
  });
  // Narrow the event column so its text is clipped.
  await page.locator('.library-column-picker > summary').click();
  await page.getByRole('checkbox', { name: 'Event', exact: true }).check();
  await page.keyboard.press('Escape');
  await expect(event).toBeVisible();
  await event.hover();
  await expect(tooltip).toHaveText(entries[1].tournament);
  const player = rows.nth(1).locator('.library-match-player');
  await player.hover();
  await expect(tooltip).toHaveText(entries[1].white);
});
