import { test, expect } from './test';
import { nativeSteps } from '../src/play-sound-events';
const id = '994b5330-bf4c-4d57-88c0-111111111111';
const other = '994b5330-bf4c-4d57-88c0-222222222222';
const db = '00000000-0000-0000-0000-000000000003';
const board = [
  0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 2,
  0,
];
const saved = (sessionId = id) => ({
  id: sessionId,
  version: 6,
  human_player: 0,
  human_name: 'Library human',
  match_length: 7,
  bearoff_direction: 'left',
  engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 2 },
  position: {
    id: sessionId,
    match_uuid: sessionId,
    ordinal: 0,
    game_no: 1,
    ply: 0,
    player: 0,
    dice: [3, 1],
    board,
    position_id: '4HPwATDgc/ABMA',
    gnubg_match_id: 'cInlAAAAAAAE',
    notation: '',
  },
  phase: 'move',
  can_double: false,
  legal_moves: [{ notation: '8/5 6/5', steps: nativeSteps('8/5 6/5', 0) }],
  history: [],
  scores: [2, 1],
  game_no: 2,
  winner: null,
});
const row = (sessionId: string | null = id, phase: string | null = 'move') => ({
  id: '994b5330-bf4c-4d57-88c0-333333333333',
  match_no: 432,
  source_available: true,
  filename: 'bot.sgf',
  format: 'sgf',
  white: 'Library human',
  black: 'GNU Backgammon',
  match_length: 7,
  position_count: 2,
  created_at: '2026-11-09T10:00:00Z',
  database_id: db,
  display_name: 'Library bot match',
  played_on: '2026-11-09',
  tournament: null,
  round: null,
  place: null,
  result_state: 'unfinished',
  white_score: 2,
  black_score: 1,
  white_pr: null,
  black_pr: null,
  pr_method: null,
  pr_profile: null,
  analysis_status: null,
  game_count: 2,
  tags: [],
  comment: '',
  version: 0,
  play_session_id: sessionId,
  play_phase: phase,
});
test('bot matches resume from Match Database and Playing has no Saved Matches list', async ({
  page,
}) => {
  let reads = 0;
  await page.addInitScript((last) => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.play-session', last);
  }, other);
  await page.route('**/api/play/sessions', (route) => {
    reads++;
    return route.fulfill({ json: [] });
  });
  await page.route('**/api/play/sessions/*', (route) =>
    route.fulfill({ json: saved(route.request().url().split('/').pop()) }),
  );
  await page.route('**/api/databases', (route) =>
    route.fulfill({
      json: [
        {
          id: db,
          name: 'Bot Matches',
          description: 'Played against the bot',
          version: 0,
          is_default: false,
          created_at: '2026-11-09T10:00:00Z',
          updated_at: '2026-11-09T10:00:00Z',
          match_count: 1,
        },
      ],
    }),
  );
  await page.route('**/api/library/matches?*', (route) =>
    route.fulfill({ json: { items: [row()], next_cursor: null } }),
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(page.locator('.play-board')).toBeVisible();
  await expect(page.getByText('Saved matches', { exact: true })).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: 'Refresh matches', exact: true }),
  ).toHaveCount(0);
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await expect(page.locator('.database-table')).toContainText('Library human');
  expect(
    await page
      .locator('.library-row-actions')
      .first()
      .evaluate((el) => {
        const cell = el.parentElement!.getBoundingClientRect();
        return [...el.children].every((child) => {
          const box = child.getBoundingClientRect();
          return box.left >= cell.left && box.right <= cell.right;
        });
      }),
  ).toBe(true);
  await page
    .getByRole('button', {
      name: 'Resume match: Library bot match',
      exact: true,
    })
    .click();
  await expect(
    page.getByRole('tab', { name: 'Playing', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect
    .poll(() =>
      page.evaluate(() => localStorage.getItem('bgtutor.play-session')),
    )
    .toBe(id);
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(page.locator('.play-board')).toBeVisible();
  await expect(page.locator('.play-match-heading')).toContainText('2 : 1');
  expect(reads).toBe(0);
});
for (const [label, sessionId, phase] of [
  ['completed', id, 'match_over'],
  ['copy', null, null],
] as const)
  test(`${label} matches have no resume action`, async ({ page }) => {
    await page.addInitScript(() =>
      localStorage.setItem('bgtutor.locale', 'en'),
    );
    await page.route('**/api/library/matches?*', (route) =>
      route.fulfill({
        json: { items: [row(sessionId, phase)], next_cursor: null },
      }),
    );
    await page.goto('/');
    await page
      .getByRole('tab', { name: 'Match database', exact: true })
      .click();
    await expect(page.locator('.database-table')).toContainText(
      'Library human',
    );
    await expect(
      page.getByRole('button', { name: /Resume match:/ }),
    ).toHaveCount(0);
  });

for (const retry of [false, true])
  test(`explicit resume supersedes a hung ${retry ? 'retry' : 'automatic restore'}`, async ({
    page,
  }) => {
    let reads = 0;
    let release = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    await page.addInitScript((last) => {
      localStorage.setItem('bgtutor.locale', 'en');
      localStorage.setItem('bgtutor.play-session', last);
    }, other);
    await page.route(`**/api/play/sessions/${other}`, async (route) => {
      if (retry && ++reads === 1) {
        await route.fulfill({
          status: 503,
          json: { error: 'Read temporarily failed' },
        });
        return;
      }
      await gate;
      await route.fulfill({ json: saved(other) }).catch(() => {});
    });
    await page.route(`**/api/play/sessions/${id}`, (route) =>
      route.fulfill({ json: saved() }),
    );
    await page.route('**/api/library/matches?*', (route) =>
      route.fulfill({ json: { items: [row()], next_cursor: null } }),
    );
    try {
      await page.goto('/');
      await page.getByRole('tab', { name: 'Playing', exact: true }).click();
      if (retry)
        await page
          .getByRole('button', { name: 'Reload saved state', exact: true })
          .click();
      await expect(page.locator('.play-workspace')).toHaveAttribute(
        'aria-busy',
        'true',
      );
      await page
        .getByRole('tab', { name: 'Match database', exact: true })
        .click();
      await page
        .getByRole('button', {
          name: 'Resume match: Library bot match',
          exact: true,
        })
        .click();
      await expect(page.locator('.play-board')).toBeVisible({ timeout: 3000 });
      await expect
        .poll(() =>
          page.evaluate(() => localStorage.getItem('bgtutor.play-session')),
        )
        .toBe(id);
    } finally {
      release();
    }
  });

for (const primary of ['white', 'black'] as const) {
  test(`mode metadata never displaces the ${primary} player name in the match library`, async ({
    page,
  }) => {
    const match = {
      ...row(),
      match_no: 34,
      white: 'Hendrik',
      black: 'GNUbg',
      play_mode: 'competition',
      play_assisted: false,
      display_name: 'bot.sgf',
    };
    await page.addInitScript(
      ({ db, primary }) => {
        localStorage.setItem('bgtutor.locale', 'en');
        localStorage.setItem('bgtutor.database', db);
        if (
          primary === 'black' &&
          !localStorage.getItem('bgtutor.library.visibleColumns')
        )
          localStorage.setItem(
            'bgtutor.library.visibleColumns',
            JSON.stringify({
              version: 1,
              columns: [
                'id',
                'black',
                'black_pr',
                'match_length',
                'score',
                'played_at',
                'created_at',
                'position_count',
                'analysis_status',
                'tags',
              ],
            }),
          );
      },
      { db, primary },
    );
    await page.route('**/api/databases', (route) =>
      route.fulfill({
        json: [
          {
            id: db,
            name: 'Bot Matches',
            description: '',
            version: 0,
            is_default: true,
            created_at: '2026-11-09T00:00:00Z',
            updated_at: '2026-11-09T00:00:00Z',
            match_count: 1,
          },
        ],
      }),
    );
    await page.route('**/api/library/matches?*', (route) =>
      route.fulfill({ json: { items: [match], next_cursor: null } }),
    );
    await page.goto('/');
    await page
      .getByRole('tab', { name: 'Match database', exact: true })
      .click();
    await expect(page.locator('th[data-column="play_mode"]')).toContainText(
      'Play mode',
    );
    const mode = page.locator('.library-play-mode-cell');
    await expect(mode).toHaveText('Competition · Independent');
    await expect(page.locator('.match-card .library-play-mode')).toHaveCount(0);
    const name = page.locator('.database-table .library-match-player');
    await expect(name).toHaveText(match[primary]);
    const size = await name.evaluate((el) => ({
      width: el.getBoundingClientRect().width,
      text: el.scrollWidth,
    }));
    expect(size.width).toBeGreaterThanOrEqual(size.text - 1);
    expect(size.width).toBeGreaterThan(30);
    await page.locator('.database-table tbody tr').focus();
    await expect(page.getByRole('tooltip')).toContainText(
      'Competition · Independent',
    );
    await page.screenshot({
      path: `/tmp/bgtutor-match-name-${primary}.png`,
      fullPage: true,
    });
    await page.locator('.library-column-picker > summary').click();
    await page
      .getByRole('checkbox', { name: 'Play mode', exact: true })
      .uncheck();
    await expect(mode).toHaveCount(0);
    await page.reload();
    await page
      .getByRole('tab', { name: 'Match database', exact: true })
      .click();
    await expect(page.locator('.library-match-player')).toHaveText(
      match[primary],
    );
    await expect(page.locator('th[data-column="play_mode"]')).toHaveCount(0);
  });
}
