import { test, expect } from './test';
const id = 'dbbd7d27-41a3-4003-b5d4-e86309f55f64';
const result = {
  winner: 1,
  points: 4,
  cube: 2,
  multiplier: 2,
  reason: 'gammon',
  scores_after: [6, 7],
};
const position = {
  id: '81315c8f-14c1-435f-89ac-07d7c7439eda',
  match_uuid: id,
  ordinal: 224,
  game_no: 6,
  ply: 48,
  player: 1,
  dice: [6, 6],
  board: [
    0, 3, 0, 2, 3, 4, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, -3,
    0,
  ],
  position_id: 'Z/eGAAAHAAAAAA',
  gnubg_match_id: 'QQn7AGAAGAAE',
  notation: '1/off 1/off 1/off',
};
const match = {
  id,
  source_available: true,
  filename: 'Hendrik-Intermediate.xg',
  format: 'xg',
  white: 'Hendrik',
  black: 'Intermediate',
  match_length: 7,
  position_count: 1,
  created_at: '2026-11-10T00:00:00Z',
};
for (const width of [1440, 390])
  test(`analysis shows game result below final move at ${width}px`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1100 });
    await page.addInitScript((id) => {
      localStorage.setItem('neondojo.locale', 'de');
      localStorage.setItem(
        'neondojo.workspace',
        JSON.stringify({ matchId: id, gameNo: 6 }),
      );
    }, id);
    await page.route(`**/api/matches/${id}`, (r) => r.fulfill({ json: match }));
    await page.route(`**/api/matches/${id}/metadata`, (r) =>
      r.fulfill({
        json: {
          ...match,
          play_mode: null,
          play_phase: null,
          play_session_id: null,
        },
      }),
    );
    await page.route(`**/api/matches/${id}/games`, (r) =>
      r.fulfill({
        json: [
          {
            game_no: 6,
            position_count: 1,
            white_score: 6,
            black_score: 3,
            crawford: false,
            result,
          },
        ],
      }),
    );
    await page.route('**/api/positions?*', (r) =>
      r.fulfill({ json: { items: [position], next_cursor: null } }),
    );
    await page.route(`**/api/matches/${id}/dice?*`, (r) =>
      r.fulfill({ json: { total: 0, rolls: [] } }),
    );
    await page.route(`**/api/matches/${id}/move-assessments?*`, (r) =>
      r.fulfill({
        json: { match_id: id, game_no: 6, job_id: null, items: [] },
      }),
    );
    await page.goto('/');
    const summary = page.locator(
      '.analysis-workspace [data-testid="game-result"]',
    );
    await expect(summary).toContainText('Intermediate gewinnt 4 Punkte');
    await expect(summary).toContainText('Gammon (2) × Cube 2 = 4 Punkte');
    await expect(summary).toContainText('Hendrik 6 : 7 Intermediate');
    await expect(
      page.locator('.analysis-workspace .move-history-row'),
    ).toHaveCount(1);
    expect(
      await summary.evaluate((el) =>
        el.previousElementSibling?.getAttribute('data-position-id'),
      ),
    ).toBe(position.id);
    const moveBox = await page
      .locator('.analysis-workspace .move-history-row')
      .last()
      .boundingBox();
    const resultBox = await summary.boundingBox();
    expect(resultBox!.y).toBeGreaterThanOrEqual(moveBox!.y + moveBox!.height);
    await page.screenshot({
      path: `/tmp/neondojo-game-result-analysis-${width}.png`,
      fullPage: true,
    });
  });

test('playing shows each completed game result at its history boundary', async ({
  page,
}) => {
  const state = {
    id,
    version: 1,
    human_player: 0,
    human_name: 'Hendrik',
    match_length: 7,
    bearoff_direction: 'right',
    engine: { id: 'gnubg', version: 'GNU Backgammon native', plies: 1 },
    play_mode: 'competition',
    position,
    phase: 'match_over',
    legal_moves: [],
    can_double: false,
    scores: [6, 7],
    game_no: 6,
    winner: 1,
    history: [
      { player: 0, dice: [], notation: 'double' },
      { player: 1, dice: [], notation: 'drop' },
      { player: 1, dice: [6, 6], notation: position.notation },
    ],
    game_results: [
      {
        game_no: 1,
        history_index: 1,
        result: {
          winner: 0,
          points: 1,
          cube: 1,
          multiplier: 1,
          reason: 'drop',
          scores_after: [1, 0],
        },
      },
      { game_no: 6, history_index: 2, result },
    ],
  };
  await page.addInitScript((id) => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.play-session', id);
    localStorage.setItem('neondojo.play-sounds', 'off');
  }, id);
  await page.route(`**/api/play/sessions/${id}`, (r) =>
    r.fulfill({ json: state }),
  );
  await page.goto('/');
  await page.locator('#workspace-tab-play').click();
  const summaries = page.locator('.play-history [data-testid="game-result"]');
  await expect(summaries).toHaveCount(2);
  await expect(summaries.first()).toContainText('Hendrik wins 1 point');
  await expect(summaries.last()).toContainText('Bot wins 4 points');
  await expect(summaries.last()).toContainText(
    'Gammon (2) × Cube 2 = 4 points',
  );
  expect(
    await summaries
      .first()
      .evaluate((el) =>
        el.previousElementSibling?.getAttribute('data-play-history-index'),
      ),
  ).toBe('1');
  expect(
    await summaries
      .last()
      .evaluate((el) =>
        el.previousElementSibling?.getAttribute('data-play-history-index'),
      ),
  ).toBe('2');
  await expect(page.locator('.play-history .move-history-row')).toHaveCount(3);
});
