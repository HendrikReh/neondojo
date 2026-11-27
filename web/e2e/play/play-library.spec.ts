import { seedLocale } from '../helpers/navigation';
import { fulfillJson, fulfillJsonList } from '../helpers/api-fixtures';
import type { components } from '../../src/generated-api';
import { databaseMatchFixture } from '../helpers/match-fixtures';
import { playSessionFixture } from '../helpers/play-session-fixture';
import { eitherOrder } from '../helpers/play-turn-fixtures';
import { rollDice, playMenuItem } from '../helpers/play-board-actions';
import { test, expect } from '../helpers/test';
import { nativeSteps } from '../../src/shared/board/play-sound-events';
import { historyFixture } from '../helpers/play-history-review-helpers';
import { profileBackend } from '../helpers/profile-backend';
const id = '994b5330-bf4c-4d57-88c0-111111111111';
const other = '994b5330-bf4c-4d57-88c0-222222222222';
const db = '00000000-0000-0000-0000-000000000003';
const saved = (sessionId = id): components['schemas']['PlaySession'] =>
  playSessionFixture({
    id: sessionId,
    version: 6,
    human_name: 'Library human',
    bearoff_direction: 'left',
    legal_turns: eitherOrder([3, 1], '8/5 6/5'),
    legal_moves: [{ notation: '8/5 6/5', steps: nativeSteps('8/5 6/5', 0) }],
    scores: [2, 1],
    game_no: 2,
  });
const row = (
  sessionId: string | null = id,
  phase: string | null = 'move',
): components['schemas']['DatabaseMatch'] =>
  databaseMatchFixture({
    id: '994b5330-bf4c-4d57-88c0-333333333333',
    match_no: 432,
    filename: 'bot.sgf',
    white: 'Library human',
    black: 'GNU Backgammon',
    position_count: 2,
    created_at: '2026-11-09T10:00:00Z',
    database_id: db,
    display_name: 'Library bot match',
    played_on: '2026-11-09',
    result_state: 'unfinished',
    white_score: 2,
    black_score: 1,
    game_count: 2,
    play_session_id: sessionId,
    play_phase: phase,
  });
test('German Playing setup has one New match heading and a localized paused collection', async ({
  page,
}) => {
  await seedLocale(page, 'de');
  await page.goto('/');
  const play = page.getByRole('tabpanel', { name: 'Spielen', exact: true });
  await page.getByRole('tab', { name: 'Spielen', exact: true }).click();
  await expect(
    play.getByRole('heading', { name: 'Neues Match', exact: true }),
  ).toHaveCount(1);
  await expect(
    play.getByRole('button', { name: 'Neues Match', exact: true }),
  ).toHaveCount(0);
  await expect(
    play.getByRole('button', { name: 'Match starten', exact: true }),
  ).toBeVisible();
  await play
    .getByRole('button', { name: 'Pausierte Matches', exact: true })
    .click();
  await expect(
    page.locator('.database-collections button[aria-pressed="true"]'),
  ).toContainText('Pausierte Matches');
});
test('bot matches resume from Match Database and Playing has no Saved Matches list', async ({
  page,
}) => {
  let reads = 0;
  await page.addInitScript((last) => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.play-session', last);
  }, other);
  await page.route('**/api/play/sessions', (route) => {
    reads++;
    return fulfillJsonList<'PlaySession'>(route, []);
  });
  await page.route('**/api/play/sessions/*', (route) =>
    fulfillJson<'PlaySession'>(
      route,
      saved(route.request().url().split('/').pop()),
    ),
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
          is_system: true,
          created_at: '2026-11-09T10:00:00Z',
          updated_at: '2026-11-09T10:00:00Z',
          match_count: 1,
        },
      ],
    }),
  );
  await page.route('**/api/library/matches?*', (route) =>
    fulfillJson<'LibraryPage'>(route, { items: [row()], next_cursor: null }),
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
  await page.locator('.database-table tbody tr').getByRole('checkbox').check();
  await page
    .locator('.library-batch-bar')
    .getByRole('button', {
      name: 'Resume match',
      exact: true,
    })
    .click();
  await expect(
    page.getByRole('tab', { name: 'Playing', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect
    .poll(() =>
      page.evaluate(() => localStorage.getItem('neondojo.play-session')),
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
  test(`${label} matches disable the resume action`, async ({ page }) => {
    await page.addInitScript(() =>
      localStorage.setItem('neondojo.locale', 'en'),
    );
    await page.route('**/api/library/matches?*', (route) =>
      fulfillJson<'LibraryPage'>(route, {
        items: [row(sessionId, phase)],
        next_cursor: null,
      }),
    );
    await page.goto('/');
    await page
      .getByRole('tab', { name: 'Match database', exact: true })
      .click();
    await expect(page.locator('.database-table')).toContainText(
      'Library human',
    );
    await page
      .locator('.database-table tbody tr')
      .getByRole('checkbox')
      .check();
    await expect(
      page
        .locator('.library-batch-bar')
        .getByRole('button', { name: 'Resume match', exact: true }),
    ).toBeDisabled();
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
      localStorage.setItem('neondojo.locale', 'en');
      localStorage.setItem('neondojo.play-session', last);
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
      await fulfillJson<'PlaySession'>(route, saved(other)).catch(() => {});
    });
    await page.route(`**/api/play/sessions/${id}`, (route) =>
      fulfillJson<'PlaySession'>(route, saved()),
    );
    await page.route('**/api/library/matches?*', (route) =>
      fulfillJson<'LibraryPage'>(route, { items: [row()], next_cursor: null }),
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
        .locator('.database-table tbody tr')
        .getByRole('checkbox')
        .check();
      await page
        .locator('.library-batch-bar')
        .getByRole('button', {
          name: 'Resume match',
          exact: true,
        })
        .click();
      await expect(page.locator('.play-board')).toBeVisible({ timeout: 3000 });
      await expect
        .poll(() =>
          page.evaluate(() => localStorage.getItem('neondojo.play-session')),
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
    const match: components['schemas']['DatabaseMatch'] = {
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
        localStorage.setItem('neondojo.locale', 'en');
        localStorage.setItem('neondojo.database', db);
        if (
          primary === 'black' &&
          !localStorage.getItem('neondojo.library.visibleColumns')
        )
          localStorage.setItem(
            'neondojo.library.visibleColumns',
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
            is_system: true,
            created_at: '2026-11-09T00:00:00Z',
            updated_at: '2026-11-09T00:00:00Z',
            match_count: 1,
          },
        ],
      }),
    );
    await page.route('**/api/library/matches?*', (route) =>
      fulfillJson<'LibraryPage'>(route, { items: [match], next_cursor: null }),
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
    await expect(page.locator('.database-table tbody tr')).toHaveAttribute(
      'aria-description',
      /Competition · Independent/,
    );
    await page.screenshot({
      path: test.info().outputPath(`match-name-${primary}.png`),
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

test('a paused match resumes from Paused Matches at its saved decision', async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  const profile = await profileBackend(page);
  const { saved: session } = await historyFixture(page);
  const pausedDb = '00000000-0000-0000-0000-000000000004';
  const match = row(session.id, 'roll');
  const checkpoint: components['schemas']['PlaySession'] = {
    ...session,
    phase: 'roll',
    paused: false,
  };
  Object.assign(session, checkpoint);
  await page.route('**/api/databases', (route) =>
    route.fulfill({
      json: [
        {
          id: '00000000-0000-0000-0000-000000000001',
          name: 'Unsorted',
          description: '',
          version: 0,
          is_default: true,
          is_system: true,
          created_at: '2026-11-09T10:00:00Z',
          updated_at: '2026-11-09T10:00:00Z',
          match_count: 0,
        },
        {
          id: pausedDb,
          name: 'Paused Matches',
          description: '',
          version: 0,
          is_default: false,
          is_system: true,
          created_at: '2026-11-09T10:00:00Z',
          updated_at: '2026-11-09T10:00:00Z',
          match_count: Number(checkpoint.paused),
        },
      ],
    }),
  );
  await page.route('**/api/library/matches?*', (route) => {
    const scope = new URL(route.request().url()).searchParams.get(
      'database_id',
    );
    return fulfillJson<'LibraryPage'>(route, {
      items:
        checkpoint.paused && scope === pausedDb
          ? [{ ...match, database_id: pausedDb, play_paused: true }]
          : [],
      next_cursor: null,
    });
  });
  const actions: string[] = [];
  await page.route(
    `**/api/play/sessions/${session.id}/actions`,
    async (route) => {
      const body = route.request().postDataJSON();
      actions.push(body.action);
      expect(body).toEqual({
        version: checkpoint.version,
        action: checkpoint.paused ? 'resume' : 'pause',
      });
      checkpoint.paused = !checkpoint.paused;
      checkpoint.version++;
      Object.assign(session, checkpoint);
      await fulfillJson<'PlaySession'>(route, checkpoint);
    },
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const toolbar = page.getByTestId('play-board-toolbar');
  await expect(rollDice(page)).toBeVisible();
  await page.getByRole('button', { name: 'Pause match', exact: true }).click();
  await expect(page.getByText('Match paused', { exact: true })).toBeVisible();
  await expect(
    page.getByRole('button', { name: 'Resume match', exact: true }),
  ).toBeFocused();
  await expect(toolbar).toHaveCount(0);
  await page.screenshot({ path: testInfo.outputPath('paused-playing.png') });
  await (await playMenuItem(page, 'Paused Matches')).click();
  await expect(
    page.getByRole('tab', { name: 'Match database', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await expect(
    page.getByRole('button', { name: 'Empty database', exact: true }),
  ).toHaveCount(0);
  await expect(
    page
      .getByRole('combobox', { name: 'Import into' })
      .locator('option:checked'),
  ).toHaveText('Unsorted');
  const matchRow = page
    .getByRole('row')
    .filter({ hasText: 'Library bot match' });
  await expect(matchRow).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath('paused-library.png') });
  await matchRow.focus();
  await matchRow.press('Enter');
  await expect(page.getByText('Match paused', { exact: true })).toBeVisible();
  expect(actions).toEqual(['pause']);
  await (await playMenuItem(page, 'Paused Matches')).click();
  await matchRow.getByRole('checkbox').check();
  await page
    .locator('.library-batch-bar')
    .getByRole('button', {
      name: 'Resume match',
      exact: true,
    })
    .click();
  await expect(toolbar).toBeVisible();
  await expect(rollDice(page)).toBeEnabled();
  expect(actions).toEqual(['pause', 'resume']);
  expect(profile.calls.filter(({ method }) => method !== 'GET')).toEqual([]);
});

test('pausing from the checker board keeps the confirmed turn checkpoint', async ({
  page,
}) => {
  const { saved: session, point } = await historyFixture(page);
  const actions: string[] = [];
  await page.route(
    `**/api/play/sessions/${session.id}/actions`,
    async (route) => {
      const body = route.request().postDataJSON();
      actions.push(body.action);
      expect(body.version).toBe(session.version);
      Object.assign(session, {
        paused: body.action === 'pause',
        version: session.version + 1,
      });
      await fulfillJson<'PlaySession'>(route, session);
    },
  );
  const undo = page.getByRole('button', { name: 'Undo step', exact: true });
  await point(13).click();
  await expect(undo).toBeEnabled();
  const pause = page.getByRole('button', { name: 'Pause match', exact: true });
  const warning = page.getByRole('dialog', {
    name: 'Pause match',
    exact: true,
  });
  await pause.click();
  await expect(warning).toContainText(
    'Unconfirmed checker steps will be discarded.',
  );
  expect(actions).toEqual([]);
  await warning.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(warning).toHaveCount(0);
  await expect(pause).toBeFocused();
  await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
  expect(actions).toEqual([]);
  await pause.click();
  await warning
    .getByRole('button', { name: 'Pause match', exact: true })
    .click();
  await expect(page.getByText('Match paused', { exact: true })).toBeVisible();
  await expect(undo).toHaveCount(0);
  await page.getByRole('button', { name: 'Resume match', exact: true }).click();
  await expect(undo).toBeDisabled();
  await expect(page.locator('.play-selected-steps')).toHaveCount(0);
  expect(actions).toEqual(['pause', 'resume']);
});
