import { rollDice } from './play-board-actions';
import { test, expect } from './test';
import type { PlaySession } from '../src/play-api';
import { applySteps } from '../src/play-state';
const id = 'f13dbce7-dae6-42c9-a63b-4967f82db47b';
for (const config of [
  {
    human: 0,
    direction: 'right' as const,
    theme: 'forest',
    die: 0,
    key: null,
    width: 1800,
    height: 1169,
  },
  {
    human: 0,
    direction: 'left' as const,
    theme: 'bgblitz:bg-earth',
    die: 1,
    key: null,
    width: 2560,
    height: 1440,
  },
  {
    human: 1,
    direction: 'right' as const,
    theme: 'bgblitz:bg-earth',
    die: 0,
    key: 'Enter',
    width: 3840,
    height: 2160,
  },
  {
    human: 1,
    direction: 'left' as const,
    theme: 'forest',
    die: 1,
    key: ' ',
    width: 1440,
    height: 1100,
  },
])
  test(`the dice pair confirms exactly once after legal completion ${config.human}/${config.direction}/${config.die}/${config.key ?? 'mouse'}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: config.width, height: config.height });
    const absolute = (point: number) =>
      config.human === 0 ? point : 25 - point;
    const board = Array<number>(26).fill(0);
    board[absolute(13)] = config.human === 0 ? 1 : -1;
    let state: PlaySession = {
      id,
      version: 0,
      human_player: config.human,
      human_name: 'Player',
      match_length: 7,
      bearoff_direction: config.direction,
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
      legal_moves: [
        { notation: '13/8', steps: [{ from: absolute(13), to: absolute(8) }] },
      ],
      history: [],
      scores: [0, 0],
      game_no: 1,
      winner: null,
      opening: null,
    };
    const writes: unknown[] = [];
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    await page.addInitScript(
      ({ id, theme }) => {
        localStorage.setItem('bgtutor.play-session', id);
        localStorage.setItem('bgtutor.theme', theme);
      },
      { id, theme: config.theme },
    );
    await page.route('**/api/play/sessions', (route) =>
      route.fulfill({ json: [state] }),
    );
    await page.route(`**/api/play/sessions/${id}`, (route) =>
      route.fulfill({ json: state }),
    );
    await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
      writes.push(route.request().postDataJSON());
      await gate;
      state = {
        ...state,
        version: 1,
        phase: 'roll',
        position: {
          ...state.position,
          board: applySteps(board, state.legal_moves[0].steps, config.human),
          dice: [],
        },
        legal_moves: [],
        history: [{ player: config.human, dice: [3, 2], notation: '13/8' }],
      };
      return route.fulfill({ json: state });
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    const die = page.locator(`[data-play-die-index="${config.die}"]`),
      point = (n: number) =>
        page.locator(`[data-play-point="${absolute(n)}"] .play-point`);
    const pair = page.locator('.play-board [data-dice-action="confirm"]');
    const artwork = page.locator(
      '.play-board [data-play-die-index] .play-die-artwork',
    );
    const toolbarConfirm = page
      .getByTestId('play-board-toolbar')
      .getByRole('button', {
        name: 'Confirm turn',
        exact: true,
      });
    await expect(toolbarConfirm).toBeDisabled();
    await die.click();
    await expect(die).toHaveCSS('outline-style', 'none');
    await expect(die).toHaveCSS('filter', 'none');
    expect(writes).toEqual([]);
    await page.locator(`[data-checker-hover-point="${absolute(13)}"]`).hover();
    await expect(
      page.locator(`[data-play-destination="${absolute(11)}"]`),
    ).toBeVisible();
    await die.click();
    await point(13).click();
    await expect(page.locator('.play-workspace')).toHaveAttribute(
      'aria-busy',
      'false',
    );
    await die.click({ force: true });
    expect(writes).toEqual([]);
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    await point(10).click();
    await expect(pair).toBeEnabled();
    await expect(toolbarConfirm).toBeEnabled();
    await page.mouse.move(0, 0);
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
    await page.mouse.move(gap.x, gap.y);
    await expect(artwork.first()).toHaveCSS('opacity', '0.55');
    await expect(artwork.last()).toHaveCSS('opacity', '0.55');
    await expect(pair).toHaveCount(1);
    await expect(pair).toHaveRole('button');
    await expect(pair).toHaveAttribute('tabindex', '0');
    await expect(pair).toHaveCSS('outline-style', 'none');
    for (const [index, face] of faces.entries()) {
      await expect(face).toHaveCSS('filter', /drop-shadow\(/);
      await expect
        .poll(async () => (await face.boundingBox())!.y)
        .toBeLessThan(resting[index].y - 0.5);
    }
    for (const face of faces) {
      await face.hover();
      await expect(artwork.first()).toHaveCSS('opacity', '0.55');
      await expect(artwork.last()).toHaveCSS('opacity', '0.55');
    }
    await page.mouse.move(gap.x, gap.y);
    await page.screenshot({ path: test.info().outputPath('confirm-dice.png') });
    await expect(pair).toHaveAccessibleDescription(/13\/10 10\/8/);
    expect(writes).toEqual([]);
    await page.getByRole('button', { name: 'Undo step', exact: true }).click();
    await expect(page.locator('.play-workspace')).toHaveAttribute(
      'aria-busy',
      'false',
    );
    await expect(die).toHaveAttribute('data-dice-action', 'none');
    await expect(toolbarConfirm).toBeDisabled();
    await page.mouse.click(gap.x, gap.y);
    expect(writes).toEqual([]);
    await point(10).click();
    await expect(pair).toBeEnabled();
    if (config.key) {
      await page.keyboard.press('Tab');
      await pair.focus();
      await page.mouse.move(gap.x, gap.y);
      await expect(pair).toHaveCSS('outline-style', 'none');
      for (const face of faces) {
        await expect(face).toHaveCSS('outline-style', 'solid');
      }
      await pair.press(config.key);
      await page.keyboard.press(config.key);
    } else await page.mouse.dblclick(gap.x, gap.y);
    await expect.poll(() => writes.length).toBe(1);
    expect(writes).toEqual([{ version: 0, action: 'move', move: 0 }]);
    release();
    await expect(rollDice(page)).toBeVisible();
    expect(writes).toHaveLength(1);
  });

for (const mode of ['double', 'double_archive', 'single_usable'] as const)
  test(`dice confirm a complete ${mode} turn`, async ({ page }) => {
    const board = Array<number>(26).fill(0),
      double = mode.startsWith('double');
    if (double) board[6] = 4;
    else {
      board[25] = 1;
      board[19] = -2;
      board[18] = -2;
    }
    let state: PlaySession = {
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
        dice: double ? [1, 1] : [6, 1],
        board,
        position_id: '4HPwATDgc/ABMA',
        gnubg_match_id: 'cInlAAAAAAAE',
        notation: '',
      },
      phase: 'move',
      can_double: false,
      legal_moves: [
        {
          notation: double ? '6/5(4)' : 'bar/24',
          steps: double
            ? Array.from({ length: 4 }, () => ({ from: 6, to: 5 }))
            : [{ from: 25, to: 24 }],
        },
      ],
      history: [],
      scores: [0, 0],
      game_no: 1,
      winner: null,
      opening: null,
    };
    const writes: unknown[] = [];
    await page.addInitScript(
      ({ id, theme }) => {
        localStorage.setItem('bgtutor.play-session', id);
        localStorage.setItem('bgtutor.theme', theme);
      },
      { id, theme: mode === 'double_archive' ? 'bgblitz:bg-earth' : 'forest' },
    );
    await page.route('**/api/play/sessions', (route) =>
      route.fulfill({ json: [state] }),
    );
    await page.route(`**/api/play/sessions/${id}`, (route) =>
      route.fulfill({ json: state }),
    );
    await page.route(`**/api/play/sessions/${id}/actions`, (route) => {
      writes.push(route.request().postDataJSON());
      state = {
        ...state,
        version: 1,
        phase: 'roll',
        position: { ...state.position, dice: [] },
        legal_moves: [],
      };
      return route.fulfill({ json: state });
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    if (!double) await page.locator('[data-play-die-index="0"]').click();
    for (let n = 0; n < (double ? 4 : 1); n++) {
      await page
        .locator(`[data-play-point="${double ? 6 : 25}"] .play-point`)
        .click();
      if (double) {
        const consumed = n + 1;
        const fractions = [
          Math.min(1, consumed / 2),
          Math.max(0, (consumed - 2) / 2),
        ];
        for (let index = 0; index < 2; index++) {
          const die = page.locator(`[data-play-die-index="${index}"]`);
          await expect(die).toHaveAttribute(
            'data-used-fraction',
            String(fractions[index]),
          );
          await expect(die).toHaveCSS('outline-style', 'none');
          await expect(die.locator('.play-die-active')).toHaveCount(0);
          if (fractions[index] === 0.5)
            await expect(die.locator('mask')).toHaveCount(1);
          if (fractions[index] === 1)
            await expect(die.locator('.play-die-artwork')).toHaveCSS(
              'opacity',
              '0.28',
            );
        }
        await page.screenshot({
          path: `/tmp/bgtutor-double-fade-${mode}-${consumed}.png`,
          fullPage: true,
        });
      }
      if (double && n === 0) {
        await page
          .getByRole('button', { name: 'Undo step', exact: true })
          .click();
        await expect(page.locator('[data-play-die-index="0"]')).toHaveAttribute(
          'data-used-fraction',
          '0',
        );
        await expect(page.locator('.play-workspace')).toHaveAttribute(
          'aria-busy',
          'false',
        );
        await page.locator('[data-play-point="6"] .play-point').click();
        await expect(page.locator('[data-play-die-index="0"]')).toHaveAttribute(
          'data-used-fraction',
          '0.5',
        );
      }
      if (double && n < 3)
        await expect(page.locator('[data-dice-action="confirm"]')).toHaveCount(
          0,
        );
    }
    await expect(page.locator('[data-dice-action="confirm"]')).toHaveCount(1);
    if (!double)
      await expect(page.locator('[data-play-die-index="1"]')).toHaveAttribute(
        'data-used',
        'false',
      );
    expect(writes).toEqual([]);
    await page
      .locator('[data-dice-action="confirm"] [data-play-die-index="1"]')
      .click();
    await expect(rollDice(page)).toBeVisible();
    expect(writes).toEqual([{ version: 0, action: 'move', move: 0 }]);
  });
