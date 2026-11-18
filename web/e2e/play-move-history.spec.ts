import { test, expect } from './test';
const id = '8c390ba7-76a1-44f0-a041-ae982f6d1363';
for (const config of [
  { human: 0, direction: 'right', theme: 'forest', width: 1440 },
  { human: 1, direction: 'left', theme: 'bgblitz:bg-earth', width: 1440 },
  { human: 0, direction: 'right', theme: 'forest', width: 390 },
  { human: 0, direction: 'right', theme: 'forest', width: 800 },
])
  test(`analysis-style playing history ${config.human}/${config.direction}/${config.width}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: config.width, height: 1100 });
    const history = Array.from({ length: 80 }, (_, i) => ({
      player: i % 2,
      dice: i === 2 || i === 3 ? [] : i % 2 ? [4, 2] : [3, 1],
      notation:
        i === 2 ? 'double' : i === 3 ? 'take' : i % 2 ? '8/4 6/4' : '8/5 6/5',
    }));
    const saved = {
      id,
      version: 40,
      human_player: config.human,
      human_name: 'Human player',
      match_length: 7,
      bearoff_direction: config.direction,
      engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 3 },
      position: {
        id,
        match_uuid: id,
        ordinal: 40,
        game_no: 2,
        ply: 0,
        player: config.human,
        dice: [],
        board: [
          0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0,
          0, 0, 2, 0,
        ],
        position_id: '4HPwATDgc/ABMA',
        gnubg_match_id: 'cInlAAAAAAAE',
        notation: '',
      },
      phase: 'roll',
      can_double: true,
      legal_moves: [],
      history,
      scores: [1, 0],
      game_no: 2,
      winner: null,
      opening: null,
    };
    const writes: unknown[] = [];
    await page.addInitScript(
      ({ id, theme }) => {
        localStorage.setItem('bgtutor.locale', 'en');
        localStorage.setItem('bgtutor.play-session', id);
        localStorage.setItem('bgtutor.theme', theme);
      },
      { id, theme: config.theme },
    );
    await page.route(`**/api/play/sessions/${id}`, (r) =>
      r.fulfill({ json: saved }),
    );
    await page.route(`**/api/play/sessions/${id}/actions`, (r) => {
      writes.push(r.request().postDataJSON());
      return r.fulfill({ json: saved });
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    const panel = page.locator('.play-history'),
      rows = panel.locator('.move-history-row');
    await expect(rows).toHaveCount(80);
    await expect(rows.first().locator('.move-no')).toHaveText('1');
    await expect(
      rows.first().locator('.move-history-details > strong'),
    ).toHaveText('8/5 6/5');
    await expect(rows.first().locator('.move-dice')).toHaveText('3–1');
    await expect(rows.first().locator('.player-dot')).toHaveClass(/white/);
    await expect(rows.nth(1).locator('.player-dot')).toHaveClass(/black/);
    await expect(rows.nth(2).locator('.move-dice')).toHaveCount(0);
    await expect(rows.last()).toHaveAttribute('aria-current', 'step');
    await expect(panel.locator('.section-heading')).toContainText('80');
    const list = panel.locator('.moves-list');
    await expect
      .poll(() => list.evaluate((el) => el.scrollTop))
      .toBeGreaterThan(0);
    const box = await panel.boundingBox(),
      board = await page.locator('.play-board').boundingBox();
    if (config.width > 900) {
      expect(box!.x + box!.width).toBeLessThanOrEqual(board!.x);
      expect(box!.width).toBeLessThanOrEqual(280);
      expect(board!.width).toBeGreaterThan(box!.width * 2);
      expect(
        await page.evaluate(() => document.documentElement.scrollHeight),
      ).toBeLessThan(2200);
    } else expect(box!.y).toBeGreaterThan(board!.y);
    await expect(list).toHaveCSS('display', 'block');
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(config.width);
    await expect(rows.first().locator('button')).toHaveCount(0);
    expect(writes).toEqual([]);
    await expect(
      panel.getByRole('link', { name: 'Download SGF', exact: true }),
    ).toHaveAttribute('href', `/api/play/sessions/${id}/sgf`);
    await page.screenshot({
      path: test.info().outputPath('playing-history.png'),
      fullPage: true,
    });
  });

test('history rates the corrected move and names the session analysis engine', async ({
  page,
}) => {
  const board = [
    0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0,
    2, 0,
  ];
  const snapshot = (engine_id: string, engine_version: string) => ({
    engine_id,
    engine_version,
    gnubg_path: '/opt/gnubg',
  });
  const saved = {
    id,
    version: 3,
    human_player: 0,
    human_name: 'Learner',
    match_length: 0,
    bearoff_direction: 'right',
    engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 1 },
    engine_snapshots: {
      rules: snapshot('gnubg', 'GNU Backgammon 1.08.003'),
      bot: snapshot('gnubg', 'GNU Backgammon 1.08.003'),
      analysis: snapshot(
        'bgsage',
        'Open Sage 2.0.20260907; model abc; 2ply cubeful',
      ),
    },
    play_mode: 'coach',
    analysis_plies: 2,
    tutor_threshold: 0.02,
    training_auto_analysis: true,
    pending_decision: null,
    decisions: [
      {
        id: '07aab353-8dc9-40c8-91a9-3377483240b5',
        game_no: 1,
        history_index: 0,
        action: 'move',
        move_index: 0,
        notation: '13/7 8/7',
        original_action: 'move',
        original_move_index: 8,
        original_notation: '8/2 6/5',
        warning: false,
        revealed: true,
        assisted: false,
        retry_count: 1,
        first_loss: 0.331,
        loss: 0,
        first_severity: 'very_bad',
        severity: 'none',
        alternatives: [],
        attempts: [],
        committed: true,
      },
    ],
    position: {
      id,
      match_uuid: id,
      ordinal: 1,
      game_no: 1,
      ply: 1,
      player: 0,
      dice: [],
      board,
      position_id: '4HPwATDgc/ABMA',
      gnubg_match_id: 'MAEHAAAAAAAA',
      notation: '',
    },
    phase: 'roll',
    can_double: false,
    legal_moves: [],
    history: [{ player: 0, dice: [6, 1], notation: '13/7 8/7' }],
    scores: [0, 0],
    game_no: 1,
    winner: null,
    opening: null,
  };
  await page.addInitScript((id) => {
    localStorage.setItem('bgtutor.locale', 'en');
    localStorage.setItem('bgtutor.play-session', id);
  }, id);
  await page.route(`**/api/play/sessions/${id}`, (r) =>
    r.fulfill({ json: saved }),
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const row = page.locator('.play-history .move-history-row').first();
  await expect(row.locator('.move-history-details > strong')).toHaveText(
    '13/7 8/7',
  );
  await expect(row.locator('.move-loss-value')).toHaveText('0.000');
  await expect(row).not.toHaveClass(/move-assessment-very_bad/);
  const marker = row.locator('.move-assessment-marker');
  await expect(marker).toHaveAttribute(
    'data-move-assessment',
    /Open Sage 2\.0\.20260907 · 2-ply/,
  );
  await expect(marker).not.toHaveAttribute('data-move-assessment', /GNUbg/);
  await expect(
    page.locator('.play-history .play-history-first-attempt'),
  ).toHaveText('First attempt: 8/2 6/5 · 0.331');
  await row.hover();
  await expect(page.getByRole('tooltip')).toContainText(
    'First attempt: 8/2 6/5 · Δ +0.331',
  );
});
