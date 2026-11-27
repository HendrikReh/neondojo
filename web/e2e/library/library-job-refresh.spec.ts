import {
  databaseFixture,
  databaseMatchFixture,
} from '../helpers/match-fixtures';
import { gotoWorkspace, seedStorage } from '../helpers/navigation';
import { fulfillJson } from '../helpers/api-fixtures';
import { expect, test } from '../helpers/test';

// The Match Database reloads a page with a queued or running analysis only
// when the shared job watch reports a transition, instead of every few seconds.
const databaseId = '00000000-0000-0000-0000-0000000010b5';
const matchId = '00000000-0000-0000-0001-0000000010b5';
const jobId = '00000000-0000-0000-0002-0000000010b5';
const match = databaseMatchFixture({
  id: matchId,
  database_id: databaseId,
  filename: 'queued.xg',
  format: 'xg',
  position_count: 120,
  game_count: 4,
  created_at: '2026-11-20T12:00:00Z',
});

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
  await seedStorage(page, { locale: 'en', databaseId });
  await page.route('**/api/databases', (route) =>
    route.fulfill({
      json: [
        databaseFixture({
          id: databaseId,
          name: 'Job refresh',
          created_at: match.created_at,
        }),
      ],
    }),
  );
  await page.route('**/api/library/matches?*', (route) => {
    tableReads++;
    return fulfillJson<'LibraryPage'>(route, {
      items: [{ ...match, analysis_status: status }],
      total: 1,
      next_cursor: null,
    });
  });
  await page.route(`**/api/matches/${matchId}/analysis/jobs`, (route) =>
    route.fulfill({ json: status === 'queued' ? [job()] : [] }),
  );
  await page.route(`**/api/jobs/${jobId}`, (route) =>
    route.fulfill({ json: job() }),
  );
  await page.clock.install();
  await gotoWorkspace(page, 'library');
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
