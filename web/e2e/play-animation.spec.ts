import { checkerPath, eitherOrder } from './play-turn-fixtures';
import { botDice, doubleCube, rollDice } from './play-board-actions';
import { test, expect, type Page } from './test';
import type { PlaySession } from '../src/play-api';
import { applySteps } from '../src/play-state';
declare global {
  interface Window {
    __animationSamples: {
      time: number;
      phase: string | null;
      dice: string | null;
      dialog: boolean;
      moves: { from: string | null; x: number; y: number }[];
    }[];
  }
}
const id = 'ab1c8a41-0202-44e0-888d-fd9c775d9019';
const initial = [
  0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 2,
  0,
];
function opening(
  human = 0,
  direction: 'left' | 'right' = 'right',
): PlaySession {
  const bot = 1 - human;
  return {
    id,
    version: 0,
    human_player: human,
    human_name: 'Player',
    match_length: 7,
    bearoff_direction: direction,
    engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 3 },
    position: {
      id,
      match_uuid: id,
      ordinal: 0,
      game_no: 1,
      ply: 0,
      player: bot,
      dice: [6, 1],
      board: [...initial],
      position_id: '4HPwATDgc/ABMA',
      gnubg_match_id: 'cInlAAAAAAAE',
      notation: '',
    },
    phase: 'opening_result',
    can_double: false,
    legal_moves: [],
    history: [],
    scores: [0, 0],
    game_no: 1,
    winner: null,
    opening: {
      white_die: bot === 0 ? 6 : 1,
      black_die: bot === 1 ? 6 : 1,
      winner: bot,
      rolls: [bot === 0 ? [6, 1] : [1, 6]],
    },
  };
}
function played(before: PlaySession): PlaySession {
  const bot = 1 - before.human_player,
    steps =
      bot === 0
        ? [
            { from: 13, to: 7 },
            { from: 6, to: 5 },
          ]
        : [
            { from: 12, to: 18 },
            { from: 19, to: 20 },
          ];
  return {
    ...before,
    version: 1,
    phase: 'roll',
    position: {
      ...before.position,
      board: applySteps(before.position.board, steps, bot),
      dice: [],
      player: before.human_player,
    },
    history: [{ player: bot, dice: [6, 1], notation: '13/7 6/5' }],
  };
}
async function install(
  page: Page,
  before: PlaySession,
  next: (data: { action: string }) => Promise<PlaySession> | PlaySession,
  theme = 'forest',
) {
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.addInitScript(
    ({ id, theme }) => {
      localStorage.setItem('neondojo.play-session', id);
      localStorage.setItem('neondojo.theme', theme);
    },
    { id, theme },
  );
  let saved = before;
  await page.route('**/api/play/sessions', (route) =>
    route.fulfill({ json: [saved] }),
  );
  await page.route(`**/api/play/sessions/${id}`, (route) =>
    route.fulfill({ json: saved }),
  );
  await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
    saved = await next(route.request().postDataJSON());
    return route.fulfill({ json: saved });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(page.locator('.play-board')).toBeVisible();
  await page.evaluate(() => {
    window.__animationSamples = [];
    function sample() {
      const board = document.querySelector('.play-board');
      const moves = Array.from(
        document.querySelectorAll('[data-animated-checker]'),
      ).map((node) => {
        const b = node.getBoundingClientRect();
        return { from: node.getAttribute('data-from'), x: b.x, y: b.y };
      });
      window.__animationSamples.push({
        time: performance.now(),
        phase: board?.getAttribute('data-play-animation') ?? null,
        dice: board?.getAttribute('data-animation-dice') ?? null,
        dialog: Boolean(document.querySelector('.play-workspace dialog:modal')),
        moves,
      });
      requestAnimationFrame(sample);
    }
    requestAnimationFrame(sample);
  });
}
for (const config of [
  { human: 0, direction: 'right' as const, theme: 'forest' },
  { human: 0, direction: 'left' as const, theme: 'bgblitz:bg-earth' },
  { human: 1, direction: 'right' as const, theme: 'bgblitz:bg-earth' },
  { human: 1, direction: 'left' as const, theme: 'forest' },
])
  test(`opening bot shows same dice and moves sequentially ${config.human}/${config.direction}/${config.theme}`, async ({
    page,
  }) => {
    const before = opening(config.human, config.direction),
      after = played(before),
      bot = 1 - config.human;
    await install(page, before, () => after, config.theme);
    const board = page.locator('.play-board');
    await expect(board).toHaveAttribute('data-play-animation', 'dice');
    await expect(board).toHaveAttribute('data-animation-player', String(bot));
    await expect(board).toHaveAttribute('data-animation-dice', '6-1');
    await expect(page.locator('.play-die-rolling')).toHaveCount(0);
    await expect(rollDice(page)).not.toBeVisible();
    await expect(page.locator('.play-board [data-cube]')).toHaveCSS(
      'pointer-events',
      'none',
    );
    await expect(botDice(page)).toHaveCount(1);
    await expect(botDice(page).locator('[data-used="true"]')).toHaveCount(2);
    await expect(board.locator('[data-dice="6-1"]')).toBeVisible();
    await expect(page.locator('dialog:modal')).toHaveCount(0);
    await expect(board).toHaveAttribute('data-play-animation', 'idle');
    const samples = await page.evaluate(() => window.__animationSamples);
    const firstPoint = String(bot === 0 ? 13 : 12),
      secondPoint = String(bot === 0 ? 6 : 19);
    const firstDice = samples.findIndex(
      (sample) => sample.phase === 'dice' && sample.dice === '6-1',
    );
    const firstMotion = samples.findIndex((sample) =>
      sample.moves.some((move) => move.from === firstPoint),
    );
    expect(firstDice).toBeGreaterThanOrEqual(0);
    expect(firstMotion).toBeGreaterThan(firstDice);
    expect(samples[firstMotion].time - samples[firstDice].time).toBeGreaterThan(
      120,
    );
    const trajectory = samples.flatMap((sample) =>
      sample.moves.filter((move) => move.from === firstPoint),
    );
    expect(trajectory.length).toBeGreaterThan(3);
    expect(
      Math.hypot(
        trajectory.at(-1)!.x - trajectory[0].x,
        trajectory.at(-1)!.y - trajectory[0].y,
      ),
    ).toBeGreaterThan(10);
    expect(
      samples.some((sample) =>
        sample.moves.some((move) => move.from === secondPoint),
      ),
    ).toBe(true);
    expect(
      samples
        .filter((sample) => sample.phase === 'dice' || sample.moves.length)
        .every((sample) => !sample.dialog),
    ).toBe(true);
    expect(
      (
        await page.evaluate(
          async (id) => (await fetch(`/api/play/sessions/${id}`)).json(),
          id,
        )
      ).position.board,
    ).toEqual(after.position.board);
  });
test('a blocked bot waits for the human to roll or double', async ({
  page,
}) => {
  const before = opening();
  const blocked: PlaySession = {
    ...played(before),
    position: { ...played(before).position, board: before.position.board },
    history: [{ player: 1, dice: [6, 1], notation: 'pass' }],
    can_double: true,
  };
  const rolled: PlaySession = {
    ...blocked,
    version: 2,
    phase: 'move',
    can_double: false,
    position: { ...blocked.position, dice: [3, 1] },
  };
  const writes: unknown[] = [];
  await install(page, before, (request) => {
    writes.push(request);
    return request.action === 'start_opening_move' ? blocked : rolled;
  });
  const board = page.locator('.play-board');
  await expect(board).toHaveAttribute('data-animation-player', '1');
  await expect(botDice(page)).toBeEnabled();
  await expect(doubleCube(page)).toBeEnabled();
  await expect(board).toHaveAttribute('data-play-phase', 'roll');
  expect(writes).toEqual([{ version: 0, action: 'start_opening_move' }]);

  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(botDice(page)).toBeEnabled();
  await expect(doubleCube(page)).toBeEnabled();
  expect(writes).toHaveLength(1);
  await botDice(page).click();
  await expect(board).toHaveAttribute('data-play-phase', 'move');
  await expect(board.locator('[data-dice="3-1"]')).toBeVisible();
  expect(writes).toEqual([
    { version: 0, action: 'start_opening_move' },
    { version: 1, action: 'roll' },
  ]);
});

test('human roll starts moving immediately, settles on native dice, and animates source-click placement', async ({
  page,
}) => {
  const before = opening();
  before.phase = 'roll';
  before.position.player = 0;
  before.position.dice = [];
  before.opening = null;
  const after: PlaySession = {
    ...before,
    version: 1,
    phase: 'move',
    position: { ...before.position, dice: [3, 1] },
    legal_turns: eitherOrder([3, 1], '8/5 6/5'),
    legal_moves: [
      {
        notation: '8/5 6/5',
        steps: [
          { from: 8, to: 5 },
          { from: 6, to: 5 },
        ],
      },
    ],
  };
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await install(page, before, async () => {
    await gate;
    return after;
  });
  await rollDice(page).click();
  await expect(page.locator('.play-die-rolling')).toHaveCount(2);
  const faces = await page.locator('.die-value').allTextContents();
  await page.waitForTimeout(85);
  expect(await page.locator('.die-value').allTextContents()).not.toEqual(faces);
  release();
  await expect(page.locator('.play-board')).toHaveAttribute(
    'data-play-animation',
    'idle',
  );
  await page.locator('[data-play-point="8"] .play-point').click();
  await expect(
    page.locator('[data-animated-checker="0"][data-from="8"][data-to="5"]'),
  ).toBeVisible();
  await expect(page.locator('[data-dice-action="confirm"]')).not.toBeVisible();
  await expect(page.locator('.play-board')).toHaveAttribute(
    'data-play-animation',
    'idle',
  );
  await expect(page.locator('.play-selected-steps')).toHaveText('8/5');
});
for (const cancel of ['reload', 'workspace'])
  test(`durable bot result survives ${cancel} during playback without replay`, async ({
    page,
  }) => {
    const before = opening(),
      after = played(before);
    let actions = 0;
    await install(page, before, () => {
      ++actions;
      return after;
    });
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-animation',
      'dice',
    );
    if (cancel === 'reload') await page.reload();
    else await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await expect(rollDice(page)).toBeVisible();
    await expect(page.locator('[data-animated-checker]')).toHaveCount(0);
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-animation',
      'idle',
    );
    expect(actions).toBe(1);
    expect(
      (
        await page.evaluate(
          async (id) => (await fetch(`/api/play/sessions/${id}`)).json(),
          id,
        )
      ).version,
    ).toBe(1);
  });

for (const human of [0, 1])
  test(`hit, bearing off and repeated undo visibly animates color ${human}`, async ({
    page,
  }) => {
    const before = opening(human),
      from = human === 0 ? 3 : 22,
      to = human === 0 ? 1 : 24,
      off = human === 0 ? -1 : 26,
      bar = human === 0 ? 0 : 25;
    before.phase = 'move';
    before.opening = null;
    before.position.player = human;
    before.position.dice = [2, 1];
    before.position.board = Array<number>(26).fill(0);
    before.position.board[from] = human === 0 ? 1 : -1;
    before.position.board[to] = human === 0 ? -1 : 1;
    before.legal_turns = [checkerPath([2, 1], '3/1 1/off', human)];
    before.legal_moves = [
      {
        notation: '3/1 1/off',
        steps: [
          { from, to },
          { from: to, to: off },
        ],
      },
    ];
    await install(
      page,
      before,
      () => before,
      human === 0 ? 'forest' : 'bgblitz:bg-earth',
    );
    const board = page.locator('.play-board');
    await page.locator(`[data-play-point="${from}"] .play-point`).click();
    await expect(page.locator('[data-animated-checker]')).toHaveCount(2);
    await expect(
      page.locator(`[data-animated-checker="${1 - human}"][data-to="${bar}"]`),
    ).toBeVisible();
    await expect(
      page.getByRole('button', { name: 'Undo step', exact: true }),
    ).toBeDisabled();
    await expect(board).toHaveAttribute('data-play-animation', 'idle');
    await page.locator(`[data-play-point="${to}"] .play-point`).click();
    await expect(
      page.locator(`[data-animated-checker="${human}"][data-to="${off}"]`),
    ).toBeVisible();
    await expect(board).toHaveAttribute('data-play-animation', 'idle');
    await page.getByRole('button', { name: 'Undo step', exact: true }).click();
    await expect(
      page.locator(
        `[data-animated-checker="${human}"][data-from="${off}"][data-to="${to}"]`,
      ),
    ).toBeVisible();
    await expect(board).toHaveAttribute('data-play-animation', 'idle');
    await page.getByRole('button', { name: 'Undo step', exact: true }).click();
    await expect(
      page.locator(
        `[data-animated-checker="${1 - human}"][data-from="${bar}"][data-to="${to}"]`,
      ),
    ).toBeVisible();
    await expect(board).toHaveAttribute('data-play-animation', 'idle');
    await expect(page.locator('.play-selected-steps')).toHaveCount(0);
    await expect(page.locator(`[data-play-point="${from}"]`)).toHaveAttribute(
      'data-enabled',
      'true',
    );
  });
