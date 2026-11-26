import { fulfillJsonList } from './api-fixtures';
import {
  checkerPath,
  eitherOrder,
  joinedThirteenToEight,
} from './play-turn-fixtures';
import { test, expect } from './test';
import type { PlaySession } from '../src/workspaces/play/play-api';
const id = 'f23dfdc5-465c-4217-92d8-6c034296a32c';
for (const config of [
  { human: 0, direction: 'right', theme: 'forest' },
  { human: 0, direction: 'left', theme: 'bgblitz:bg-earth' },
  { human: 1, direction: 'right', theme: 'bgblitz:bg-earth' },
  { human: 1, direction: 'left', theme: 'forest' },
])
  test(`one source-point click moves immediately without frames ${config.human}/${config.direction}/${config.theme}`, async ({
    page,
  }) => {
    const absolute = (point: number) =>
      config.human === 0 ? point : 25 - point;
    const board = Array<number>(26).fill(0);
    board[absolute(13)] = config.human === 0 ? 1 : -1;
    const saved: PlaySession = {
      id,
      version: 0,
      human_player: config.human,
      human_name: 'Player',
      match_length: 7,
      bearoff_direction: config.direction as 'left' | 'right',
      engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 3 },
      position: {
        id,
        match_uuid: id,
        ordinal: 0,
        game_no: 1,
        ply: 0,
        player: config.human,
        dice: [3, 2],
        board,
        position_id: '4HPwATDgc/ABMA',
        gnubg_match_id: 'cInlAAAAAAAE',
        notation: '',
      },
      phase: 'move',
      can_double: false,
      legal_turns: joinedThirteenToEight(config.human),
      legal_moves: [
        { notation: '13/8', steps: [{ from: absolute(13), to: absolute(8) }] },
      ],
      history: [],
      scores: [0, 0],
      game_no: 1,
      winner: null,
      opening: null,
    };
    await page.addInitScript(
      ({ id, theme }) => {
        localStorage.setItem('neondojo.play-session', id);
        localStorage.setItem('neondojo.theme', theme);
      },
      { id, theme: config.theme },
    );
    await page.route('**/api/play/sessions', (route) =>
      fulfillJsonList<'PlaySession'>(route, [saved]),
    );
    await page.route(`**/api/play/sessions/${id}`, (route) =>
      route.fulfill({ json: saved }),
    );
    const writes: unknown[] = [];
    await page.route(`**/api/play/sessions/${id}/actions`, (route) => {
      writes.push(route.request().postDataJSON());
      return route.fulfill({
        json: { ...saved, version: 1, phase: 'roll', legal_moves: [] },
      });
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    const point = (n: number) =>
      page.locator(`[data-play-point="${absolute(n)}"]`);
    await page.locator(`[data-checker-hover-point="${absolute(13)}"]`).hover();
    await expect(
      page.locator(`[data-play-destination="${absolute(10)}"]`),
    ).toBeVisible();
    // Click empty space on the source point, rather than selecting a checker then a target.
    await point(13).locator('.play-point').click();
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    await expect(page.locator('.play-checker-selected')).toHaveCount(0);
    await point(10).focus();
    expect(
      await point(10).evaluate((node) => getComputedStyle(node).outlineStyle),
    ).toBe('none');
    expect(
      await point(10)
        .locator('.play-point')
        .evaluate((node) => getComputedStyle(node).stroke),
    ).toBe('rgba(0, 0, 0, 0)');
    await point(13).click({ force: true });
    await point(8).click({ force: true });
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    await point(10).press('Enter');
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10 10/8');
    await expect(page.locator('[data-dice-action="confirm"]')).toBeEnabled();
    expect(writes).toEqual([]);
    await page.getByRole('button', { name: 'Undo step', exact: true }).click();
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    await page.getByRole('button', { name: 'Undo step', exact: true }).click();
    await page.locator('[data-play-die-index="1"]').click();
    await point(13).press(' ');
    await expect(page.locator('.play-selected-steps')).toHaveText('13/11');
    await point(11).click();
    await expect(page.locator('.play-selected-steps')).toHaveText('13/11 11/8');
    await page.screenshot({
      path: test.info().outputPath('source-click.png'),
      fullPage: true,
    });
    await page.locator('[data-dice-action="confirm"]').click();
    expect(writes).toEqual([{ version: 0, action: 'move', move: 0 }]);
  });

for (const mode of [
  'double',
  'white_bar',
  'black_bar',
  'owned_white_bar',
  'owned_black_bar',
] as const) {
  test(`source-only repeated clicks and bar entry: ${mode}`, async ({
    page,
  }) => {
    const human = mode.includes('black_bar') ? 1 : 0;
    const owned = mode.startsWith('owned_');
    const saved: PlaySession = {
      id,
      version: 0,
      human_player: human,
      human_name: 'Player',
      match_length: 7,
      bearoff_direction: 'right',
      engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 3 },
      position: {
        id,
        match_uuid: id,
        ordinal: 0,
        game_no: 1,
        ply: 0,
        player: human,
        dice: mode === 'double' ? [1, 1] : [1, 2],
        board: Array<number>(26).fill(0),
        position_id: '4HPwATDgc/ABMA',
        gnubg_match_id: 'cInlAAAAAAAE',
        notation: '',
      },
      phase: 'move',
      can_double: false,
      legal_moves: [],
      history: [],
      scores: [0, 0],
      game_no: 1,
      winner: null,
      opening: null,
    };
    const bar = human === 0 ? 25 : 0,
      entry = human === 0 ? 24 : 1,
      last = human === 0 ? 22 : 3;
    if (mode === 'double') {
      saved.position.board[6] = 4;
      saved.legal_turns = [checkerPath([1, 1, 1, 1], '6/5 6/5 6/5 6/5')];
      saved.legal_moves = [
        {
          notation: '6/5(4)',
          steps: Array.from({ length: 4 }, () => ({ from: 6, to: 5 })),
        },
      ];
    } else if (owned) {
      saved.position.board[bar] = human === 0 ? 2 : -2;
      const key = Buffer.from(saved.position.gnubg_match_id, 'base64');
      key[0] = (key[0] & ~0x3f) | 1 | (human << 4);
      saved.position.gnubg_match_id = key.toString('base64');
      saved.legal_turns = eitherOrder([1, 2], 'bar/24 bar/23', human);
      saved.legal_moves = [
        {
          notation: 'bar/24 bar/23',
          steps: [
            { from: bar, to: entry },
            { from: bar, to: human === 0 ? 23 : 2 },
          ],
        },
      ];
    } else {
      saved.position.board[bar] = human === 0 ? 1 : -1;
      saved.position.board[entry] = human === 0 ? -1 : 1;
      saved.legal_turns = [checkerPath([1, 2], 'bar/24 24/22', human)];
      saved.legal_moves = [
        {
          notation: 'bar/24 24/22',
          steps: [
            { from: bar, to: entry },
            { from: entry, to: last },
          ],
        },
      ];
    }
    await page.addInitScript(
      (id) => localStorage.setItem('neondojo.play-session', id),
      id,
    );
    await page.route('**/api/play/sessions', (route) =>
      fulfillJsonList<'PlaySession'>(route, [saved]),
    );
    await page.route(`**/api/play/sessions/${id}`, (route) =>
      route.fulfill({ json: saved }),
    );
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    if (mode === 'double') {
      const source = page.locator('[data-play-point="6"] .play-point');
      await source.dblclick();
      await expect(page.locator('.play-selected-steps')).toHaveText('6/5 6/5');
      await expect(page.locator('[data-play-die-index="0"]')).toHaveAttribute(
        'data-used',
        'true',
      );
      await source.dblclick();
      await expect(page.locator('.play-selected-steps')).toHaveText(
        '6/5 6/5 6/5 6/5',
      );
      await expect(page.locator('[data-play-die-index="1"]')).toHaveAttribute(
        'data-used',
        'true',
      );
    } else if (owned) {
      // The idle board shown until the session loads has a cube of its own.
      const cube = page.locator('.play-live-board [data-cube]');
      await expect(cube).toHaveCSS('pointer-events', 'none');
      const box = await cube.boundingBox();
      expect(box).not.toBeNull();
      await page.mouse.click(box!.x + box!.width / 2, box!.y + box!.height / 2);
      await expect(page.locator('.play-selected-steps')).toHaveText('Bar/24');
      await page.locator(`[data-play-point="${bar}"] .play-point`).click();
      await expect(page.locator('.play-selected-steps')).toHaveText(
        'Bar/24 Bar/23',
      );
    } else {
      await page.locator(`[data-play-point="${bar}"] .play-point`).click();
      await expect(page.locator('.play-selected-steps')).toHaveText('Bar/24');
      await page.locator(`[data-play-point="${entry}"] .play-point`).click();
      await expect(page.locator('.play-selected-steps')).toHaveText(
        'Bar/24 24/22',
      );
    }
    await expect(page.locator('[data-dice-action="confirm"]')).toBeEnabled();
    await expect(page.locator('.play-checker-selected')).toHaveCount(0);
  });
}

for (const sourceKind of ['board', 'bar'] as const) {
  test(`a ${sourceKind} checker uses its only legal die even when it is shown second`, async ({
    page,
  }) => {
    const saved: PlaySession = {
      id,
      version: 0,
      human_player: 0,
      human_name: 'Player',
      match_length: 7,
      bearoff_direction: 'right',
      engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 3 },
      position: {
        id,
        match_uuid: id,
        ordinal: 0,
        game_no: 1,
        ply: 0,
        player: 0,
        dice: [3, 2],
        board: Array<number>(26).fill(0),
        position_id: '4HPwATDgc/ABMA',
        gnubg_match_id: 'cInlAAAAAAAE',
        notation: '',
      },
      phase: 'move',
      can_double: false,
      legal_moves: [],
      history: [],
      scores: [0, 0],
      game_no: 1,
      winner: null,
    };
    const from = sourceKind === 'bar' ? 25 : 13;
    const to = from - 2;
    saved.position.board[from] = 1;
    saved.position.board[from - 3] = -2;
    saved.position.board[6] = 1;
    saved.legal_moves = [
      {
        notation: sourceKind === 'bar' ? 'bar/23 6/3' : '13/11 6/3',
        steps: [
          { from, to },
          { from: 6, to: 3 },
        ],
      },
    ];
    const notation = sourceKind === 'bar' ? 'bar/23 6/3' : '13/11 6/3';
    saved.legal_turns =
      sourceKind === 'bar'
        ? [checkerPath([2, 3], notation)]
        : eitherOrder([2, 3], notation);
    // In the board case the preferred die is usable on another checker.
    if (sourceKind === 'board') {
      saved.legal_turns.push({
        ...checkerPath([3, 2], '6/3 3/1'),
        move_index: 1,
      });
      saved.legal_moves.push({
        notation: '6/3/1',
        steps: [{ from: 6, to: 1 }],
      });
    }
    await page.addInitScript(
      (id) => localStorage.setItem('neondojo.play-session', id),
      id,
    );
    await page.route(`**/api/play/sessions/${id}`, (route) =>
      route.fulfill({ json: saved }),
    );
    const writes: unknown[] = [];
    await page.route(`**/api/play/sessions/${id}/actions`, (route) => {
      writes.push(route.request().postDataJSON());
      return route.fulfill({
        json: { ...saved, version: 1, phase: 'roll', legal_moves: [] },
      });
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    const source = page.locator(`[data-checker-hover-point="${from}"]`);
    await source.hover();
    await expect(page.locator(`[data-play-destination="${to}"]`)).toBeVisible();
    await source.click();
    const first = page.locator('[data-play-die-index="0"]');
    const second = page.locator('[data-play-die-index="1"]');
    await expect(page.locator('.play-selected-steps')).toHaveText(
      sourceKind === 'bar' ? 'Bar/23' : '13/11',
    );
    await expect(first).toHaveAttribute('data-used', 'false');
    await expect(second).toHaveAttribute('data-used', 'true');
    await page.getByRole('button', { name: 'Undo step', exact: true }).click();
    await expect(second).toHaveAttribute('data-used', 'false');
    await source.click();
    await page.locator('[data-checker-hover-point="6"]').click();
    const confirm = page.locator('[data-dice-action="confirm"]');
    await expect(confirm).toBeEnabled();
    await first.click();
    expect(writes).toEqual([{ version: 0, action: 'move', move: 0 }]);
  });
}
