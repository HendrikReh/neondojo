import { expect, test } from './test';

// The Match Database reloads a page with a queued or running analysis only
// when the shared job watch reports a transition, instead of every few seconds.
const databaseId = '00000000-0000-0000-0000-0000000010b5';
const matchId = '00000000-0000-0000-0001-0000000010b5';
const jobId = '00000000-0000-0000-0002-0000000010b5';
const match = {
  match_no: 1,
  id: matchId,
  database_id: databaseId,
  filename: 'queued.xg',
  display_name: 'queued.xg',
  format: 'xg',
  white: 'Alice',
  black: 'Bob',
  match_length: 7,
  position_count: 120,
  game_count: 4,
  created_at: '2026-11-20T12:00:00Z',
  played_on: null,
  tournament: null,
  round: null,
  place: null,
  result_state: 'completed',
  white_score: 7,
  black_score: 3,
  white_pr: null,
  black_pr: null,
  pr_method: null,
  pr_profile: null,
  source_available: true,
  import_warning_count: 0,
  play_assisted: false,
  play_paused: false,
  tags: [],
  comment: '',
  version: 0,
};

test('queued library analyses refresh the table on job transitions only', async ({
  page,
}) => {
  let status: 'queued' | 'completed' = 'queued';
  let tableReads = 0;
  const job = () => ({
    id: jobId,
    kind: 'match',
    match_uuid: matchId,
    status,
    progress: status === 'completed' ? 100 : 0,
    attempts: status === 'completed' ? 1 : 0,
    completed_games: status === 'completed' ? 4 : 0,
    total_games: 4,
    plies: 2,
    game_no: null,
    created_at: '2026-11-20T12:00:00Z',
    updated_at: '2026-11-20T12:00:00Z',
  });
  await page.addInitScript((id) => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.database', id);
  }, databaseId);
  await page.route('**/api/databases', (route) =>
    route.fulfill({
      json: [
        {
          id: databaseId,
          name: 'Job refresh',
          description: '',
          is_default: true,
          is_system: false,
          version: 0,
          match_count: 1,
          created_at: match.created_at,
          updated_at: match.created_at,
        },
      ],
    }),
  );
  await page.route('**/api/library/matches?*', (route) => {
    tableReads++;
    return route.fulfill({
      json: {
        items: [{ ...match, analysis_status: status }],
        total: 1,
        next_cursor: null,
      },
    });
  });
  await page.route(`**/api/matches/${matchId}/analysis/jobs`, (route) =>
    route.fulfill({ json: status === 'queued' ? [job()] : [] }),
  );
  await page.route(`**/api/jobs/${jobId}`, (route) =>
    route.fulfill({ json: job() }),
  );
  await page.clock.install();
  await page.goto('/');
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  const row = page.locator('.database-table tbody tr');
  await expect(row).toContainText('Queued');
  const reads = tableReads;
  // The former polling read the page every 4 s.
  await page.clock.runFor(20_000);
  expect(tableReads).toBe(reads);

  status = 'completed';
  await page.clock.runFor(4_000);
  await expect(row).toContainText('Completed');
  expect(tableReads).toBe(reads + 1);
  await page.clock.runFor(20_000);
  expect(tableReads).toBe(reads + 1);
});
