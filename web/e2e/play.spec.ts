import type { components } from '../src/generated-api';
import {
  fulfillJsonList,
  fulfillJson,
  mockSettings,
  settingsFixture,
} from './api-fixtures';
import type { PlaySession } from '../src/workspaces/play/play-api';
import { eitherOrder, joinedThirteenToEight } from './play-turn-fixtures';
import { applyAndCloseSettings } from './settings-actions';
import { rollDice, doubleCube, passDice } from './play-board-actions';
import { test, expect } from './test';
const initialBoard = [
  0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 2,
  0,
];
const id = '9ef7a4aa-56c7-4b32-b7dd-78029632e879';
function session(
  human = 0,
  direction = 'right',
): components['schemas']['PlaySession'] {
  return {
    id,
    version: 0,
    human_player: human,
    human_name: 'Player',
    match_length: 7,
    bearoff_direction: direction,
    engine: {
      id: 'gnubg',
      version: 'GNU Backgammon 1.08.003 native',
      plies: 3,
    },
    position: {
      id,
      match_uuid: id,
      ordinal: 0,
      game_no: 1,
      ply: 0,
      player: human,
      dice: [3, 1],
      board: initialBoard,
      position_id: '4HPwATDgc/ABMA',
      gnubg_match_id: 'cInlAAAAAAAE',
      notation: '',
    },
    phase: 'move',
    can_double: false,
    legal_turns: eitherOrder([3, 1], '8/5 6/5', human),
    legal_moves: [
      {
        notation: '8/5 6/5',
        steps: [
          { from: 8, to: 5 },
          { from: 6, to: 5 },
        ],
      },
    ],
    history: [] as PlaySession['history'],
    scores: [0, 0],
    game_no: 1,
    winner: null,
  };
}
for (const direction of ['right', 'left'])
  test(`playing legal selection, undo, confirmation and resume ${direction}`, async ({
    page,
  }) => {
    let saved = session(0, direction);
    const writes: unknown[] = [];
    await page.route('**/api/play/sessions', (route) =>
      route.request().method() === 'POST'
        ? fulfillJson<'PlaySession'>(route, saved, 201)
        : fulfillJsonList<'PlaySession'>(route, [saved]),
    );
    await page.route(`**/api/play/sessions/${id}`, (route) =>
      route.fulfill({ json: saved }),
    );
    await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
      writes.push(route.request().postDataJSON());
      saved = {
        ...saved,
        version: saved.version + 1,
        phase: 'roll',
        legal_moves: [],
        history: [{ player: 0, dice: [3, 1], notation: '8/5 6/5' }],
      } as typeof saved;
      await route.fulfill({ json: saved });
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await page
      .getByRole('button', { name: 'Start match', exact: true })
      .click();
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-direction',
      direction,
    );
    await expect(
      page
        .locator('.play-match-engine > span')
        .first()
        .locator('.engine-label-details'),
    ).toHaveText('GNU Backgammon 1.08.003');
    const point = (n: number) => page.locator(`[data-play-point="${n}"]`);
    await expect(
      page.locator('[data-dice-action="confirm"]'),
    ).not.toBeVisible();
    await point(8).click();
    await point(6).click();
    await expect(page.locator('[data-dice-action="confirm"]')).toBeEnabled();
    await page.getByRole('button', { name: 'Undo step', exact: true }).click();
    await expect(
      page.locator('[data-dice-action="confirm"]'),
    ).not.toBeVisible();
    await point(6).press('Enter');
    await page.locator('[data-dice-action="confirm"]').click();
    expect(writes).toEqual([{ version: 0, action: 'move', move: 0 }]);
    await expect(rollDice(page)).toBeVisible();
    await page.reload();
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await expect(rollDice(page)).toBeVisible();
    await expect(page.locator('.play-history')).toContainText('8/5 6/5');
  });
test('black perspective is stable and uses archive artwork with readable labels', async ({
  page,
}) => {
  const saved = session(1, 'left');
  saved.legal_moves = [
    {
      notation: '17/20 19/20',
      steps: [
        { from: 17, to: 20 },
        { from: 19, to: 20 },
      ],
    },
  ];
  await page.addInitScript(() =>
    localStorage.setItem('neondojo.theme', 'bgblitz:bg-earth'),
  );
  await page.route('**/api/play/sessions', (route) =>
    route.request().method() === 'POST'
      ? fulfillJson<'PlaySession'>(route, saved, 201)
      : fulfillJsonList<'PlaySession'>(route, []),
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page.getByRole('radio', { name: 'Black', exact: true }).check();
  await page.getByRole('button', { name: 'Start match', exact: true }).click();
  await expect(page.locator('.play-board')).toHaveAttribute(
    'data-human-player',
    '1',
  );
  await expect(page.locator('.play-board svg.board')).toHaveClass(
    /bgblitz-board/,
  );
  await expect(page.locator('[data-play-point="17"]')).toHaveAttribute(
    'aria-label',
    'Point 8',
  );
  await page.locator('[data-play-point="17"]').click();
  await page.locator('[data-play-point="19"]').click();
  await expect(page.locator('[data-dice-action="confirm"]')).toBeEnabled();
});

test('playing identifies each engine role and exposes only the supplied checker paths', async ({
  page,
}) => {
  const saved = {
    ...session(),
    engine: { id: 'bgsage', version: 'NeonGammon 3.0.0', plies: 2 },
    engine_snapshots: {
      analysis: { engine_id: 'bgsage', engine_version: 'NeonGammon 3.0.0' },
      rules: { engine_id: 'gnubg', engine_version: 'GNU Backgammon 1.08.003' },
    },
    legal_turns: eitherOrder([3, 1], '8/5 6/5').slice(0, 1),
  };
  await page.route(`**/api/play/sessions/${id}`, (route) =>
    route.fulfill({ json: saved }),
  );
  await page.addInitScript((sessionId) => {
    localStorage.setItem('neondojo.play-session', sessionId);
  }, id);
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const engines = page.locator('.play-match-engine');
  await expect(engines).toContainText('Bot engine · NeonGammon 3.0.0 2-ply');
  await expect(engines).toContainText('Analysis engine · NeonGammon 3.0.0');
  await expect(engines).toContainText('Rules engine · GNU Backgammon 1.08.003');
  await expect(page.locator('[data-play-point="8"]')).toBeEnabled();
  await expect(page.locator('[data-play-point="6"]')).toBeDisabled();
});

test('a bot response remains owned when switching workspaces while it is thinking', async ({
  page,
}) => {
  let saved = session();
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/api/play/sessions', (route) =>
    route.request().method() === 'POST'
      ? fulfillJson<'PlaySession'>(route, saved, 201)
      : fulfillJsonList<'PlaySession'>(route, [saved]),
  );
  await page.route(`**/api/play/sessions/${id}`, (route) =>
    route.fulfill({ json: saved }),
  );
  await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
    await gate;
    saved = {
      ...saved,
      version: 1,
      phase: 'roll',
      legal_moves: [],
    } as typeof saved;
    await route.fulfill({ json: saved });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page.getByRole('button', { name: 'Start match', exact: true }).click();
  for (const from of [8, 6]) {
    await page.locator(`[data-play-point="${from}"]`).click();
  }
  await page.locator('[data-dice-action="confirm"]').click();
  const thinking = page
    .getByRole('tabpanel', { name: 'Playing', exact: true })
    .getByRole('status')
    .filter({ hasText: 'The bot is thinking…' });
  await expect(thinking).toBeVisible();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(thinking).toBeVisible();
  release();
  await expect(rollDice(page)).toBeVisible();
});

test('Settings persists the chosen bear-off direction for new matches', async ({
  page,
}) => {
  await mockSettings(page, ({ respond }) =>
    respond({
      ...settingsFixture(),
      player_name: 'Player',
      play_plies: 2,
      gnubg_path: '/test/gnubg',
      bearoff_direction: 'right',
    }),
  );
  await page.goto('/');
  await page
    .getByRole('button', { name: 'Settings', exact: true })
    .first()
    .click();
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  await dialog.getByRole('button', { name: 'Playing', exact: true }).click();
  await expect(
    dialog.getByLabel('Bear-off direction', { exact: true }),
  ).toHaveValue('right');
  await dialog
    .getByLabel('Bear-off direction', { exact: true })
    .selectOption('left');
  await applyAndCloseSettings(dialog);
  const configuration = await page.evaluate(async () => {
    const response = await fetch('/api/configuration');
    return response.json();
  });
  expect(configuration.current_configuration.appearance.bearoff_direction).toBe(
    'left',
  );
  expect(
    configuration.active_profile.configuration.appearance.bearoff_direction,
  ).toBe('right');
  await page
    .getByRole('button', { name: 'Settings', exact: true })
    .first()
    .click();
  await dialog.getByRole('button', { name: 'Playing', exact: true }).click();
  await expect(
    dialog.getByLabel('Bear-off direction', { exact: true }),
  ).toHaveValue('left');
});

test('cube responses, forced pass and next game submit only the available actions', async ({
  page,
}) => {
  let saved = {
    ...session(),
    phase: 'double_response',
    legal_moves: [],
  } as ReturnType<typeof session>;
  const writes: unknown[] = [];
  await page.route('**/api/play/sessions', (route) =>
    route.request().method() === 'POST'
      ? fulfillJson<'PlaySession'>(route, saved, 201)
      : fulfillJsonList<'PlaySession'>(route, []),
  );
  await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
    const action = route.request().postDataJSON();
    writes.push(action);
    saved = {
      ...saved,
      version: saved.version + 1,
      ...(action.action === 'take'
        ? { phase: 'move', legal_moves: [] }
        : action.action === 'pass'
          ? { phase: 'game_over', winner: 0, scores: [2, 0] }
          : { phase: 'roll', can_double: true }),
    } as typeof saved;
    await route.fulfill({ json: saved });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page.getByRole('button', { name: 'Start match', exact: true }).click();
  await expect(rollDice(page)).toHaveCount(0);
  await page.getByRole('button', { name: 'Take double', exact: true }).click();
  await passDice(page).press('Enter');
  await expect(page.getByText('Winner: Player', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Next game', exact: true }).click();
  await expect(doubleCube(page)).toBeVisible();
  expect(writes).toEqual([
    { version: 0, action: 'take' },
    { version: 1, action: 'pass' },
    { version: 2, action: 'next_game' },
  ]);
});

for (const theme of ['forest', 'bgblitz:bg-earth'])
  test(`subtle hover dots and first-die swapping at dice corners ${theme}`, async ({
    page,
  }) => {
    const saved = session();
    saved.position.board = Array(26).fill(0);
    saved.position.board[13] = 1;
    saved.position.dice = [3, 2];
    saved.legal_turns = joinedThirteenToEight();
    saved.legal_moves = [{ notation: '13/8', steps: [{ from: 13, to: 8 }] }];
    await page.addInitScript(
      (value) => localStorage.setItem('neondojo.theme', value),
      theme,
    );
    await page.route('**/api/play/sessions', (route) =>
      route.request().method() === 'POST'
        ? fulfillJson<'PlaySession'>(route, saved, 201)
        : fulfillJsonList<'PlaySession'>(route, []),
    );
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await page
      .getByRole('button', { name: 'Start match', exact: true })
      .click();
    await expect(page.locator('[data-play-destination]')).toHaveCount(0);
    const hover = () => page.locator('[data-checker-hover-point="13"]').hover();
    await hover();
    await expect(page.locator('[data-play-destination="10"]')).toBeVisible();
    await expect(page.locator('[data-play-destination="8"]')).toHaveCount(0);
    await page.locator('.play-match-format').hover();
    await expect(page.locator('[data-play-destination]')).toHaveCount(0);
    const pair = page.getByRole('button', { name: /^Swap dice order ·/ });
    await expect(pair).toHaveCount(1);
    const artwork = pair.locator('.play-die-artwork');
    await expect(artwork).toHaveCount(2);
    const faces = await artwork.all();
    const resting = (
      await Promise.all(faces.map((face) => face.boundingBox()))
    ).map((bounds) => bounds!);
    const [left, right] = [...resting].sort((a, b) => a.x - b.x);
    expect(left.x + left.width).toBeLessThan(right.x);
    const gap = {
      x: (left.x + left.width + right.x) / 2,
      y: left.y + left.height / 2,
    };
    for (const hovered of faces) {
      await hovered.hover();
      for (const face of faces) {
        await expect(face).toHaveCSS('filter', /drop-shadow\(/);
      }
    }
    await page.mouse.move(gap.x, gap.y);
    await expect(pair).toHaveCSS('outline-style', 'none');
    for (const face of faces) {
      await expect(face).toHaveCSS('filter', /drop-shadow\(/);
      await expect(face).toHaveCSS('transform', /matrix\(/);
    }
    await page.screenshot({ path: test.info().outputPath('swap-dice.png') });
    await page.mouse.click(gap.x, gap.y);
    await expect(pair).toHaveAccessibleName('Swap dice order · 2, 3');
    await pair.press('Space');
    await expect(pair).toHaveAccessibleName('Swap dice order · 3, 2');
    for (let corner = 0; corner < 4; corner++) {
      const size = resting[corner % 2];
      await page.mouse.click(
        size.x + (corner % 2 ? size.width - 2 : 2),
        size.y + (corner < 2 ? 2 : size.height - 2),
      );
      await hover();
      await expect(
        page.locator(`[data-play-destination="${corner % 2 ? 10 : 11}"]`),
      ).toBeVisible();
    }
    await page.locator('[data-checker-hover-point="13"]').click();
    await expect(page.locator('[data-play-die-index="0"]')).toHaveAttribute(
      'data-used',
      'true',
    );
    await expect(page.locator('[data-play-die-index="1"]')).toHaveAttribute(
      'aria-disabled',
      'true',
    );
    await expect(page.locator('[data-dice-action="swap"]')).toHaveCount(0);
    await page.mouse.click(gap.x, gap.y);
    await page.locator('[data-checker-hover-point="10"]').hover();
    await expect(page.locator('[data-play-destination="8"]')).toBeVisible();
    await expect(
      page.locator('[data-dice-action="confirm"]'),
    ).not.toBeVisible();
    await page.locator('[data-checker-hover-point="10"]').click();
    await expect(page.locator('[data-dice-action="confirm"]')).toBeEnabled();
    await page.getByRole('button', { name: 'Undo step', exact: true }).click();
    await expect(
      page.locator('[data-dice-action="confirm"]'),
    ).not.toBeVisible();
  });

test('a black hit after an already moved checker remains confirmable with source-only clicks', async ({
  page,
}) => {
  const saved = session(1, 'left');
  saved.position.board = Array(26).fill(0);
  for (const [point, count] of [
    [1, -4],
    [2, 1],
    [6, -1],
    [23, -1],
    [24, -9],
  ])
    saved.position.board[point] = count;
  saved.position.dice = [3, 1];
  saved.legal_turns = eitherOrder([3, 1], '24/21 24/23', 1);
  saved.legal_moves = [
    {
      notation: '24/23* 24/21',
      steps: [
        { from: 1, to: 2 },
        { from: 1, to: 4 },
      ],
    },
  ];
  await page.addInitScript(() =>
    localStorage.setItem('neondojo.theme', 'bgblitz:bg-earth'),
  );
  await page.route('**/api/play/sessions', (route) =>
    route.request().method() === 'POST'
      ? fulfillJson<'PlaySession'>(route, saved, 201)
      : fulfillJsonList<'PlaySession'>(route, []),
  );
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page.getByRole('button', { name: 'Start match', exact: true }).click();
  for (const [from, to] of [
    [1, 4],
    [1, 2],
  ]) {
    const checker = page.locator(`[data-checker-hover-point="${from}"]`);
    await checker.hover();
    await expect(page.locator(`[data-play-destination="${to}"]`)).toBeVisible();
    await checker.click();
  }
  await expect(page.locator('[data-dice-action="confirm"]')).toBeEnabled();
});
