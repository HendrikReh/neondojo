import { test, expect } from './test';
import type { PlaySession } from '../src/play-api';
import { readFileSync } from 'node:fs';
import type { BgBlitzTheme } from '../src/themes';
const archiveThemes = JSON.parse(
  readFileSync(
    new URL('../src/generated-themes.json', import.meta.url),
    'utf8',
  ),
) as BgBlitzTheme[];
const id = 'ec9b317a-615f-49b1-b1de-8cddaf2561f1';
for (const config of [
  { human: 0, direction: 'right' as const, theme: 'forest' },
  { human: 1, direction: 'left' as const, theme: 'forest' },
  { human: 0, direction: 'left' as const, theme: 'bgblitz:bg-earth' },
  { human: 1, direction: 'right' as const, theme: 'bgblitz:bg-earth' },
])
  test(`opening rolls native-themed white and black dice inside board ${config.human}/${config.direction}/${config.theme}`, async ({
    page,
  }) => {
    let state: PlaySession = {
      id,
      version: 0,
      human_player: config.human,
      human_name: 'Player',
      match_length: 7,
      bearoff_direction: config.direction,
      engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 2 },
      position: {
        id,
        match_uuid: id,
        ordinal: 0,
        game_no: 1,
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
      phase: 'opening_roll',
      can_double: false,
      legal_moves: [],
      history: [],
      scores: [0, 0],
      game_no: 1,
      winner: null,
      opening: null,
    };
    let writes = 0;
    await page.emulateMedia({ reducedMotion: 'no-preference' });
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
    await page.route(`**/api/play/sessions/${id}/actions`, (route) => {
      ++writes;
      state = {
        ...state,
        version: 1,
        phase: 'opening_result',
        position: { ...state.position, player: 0, dice: [6, 2] },
        opening: {
          white_die: 6,
          black_die: 2,
          winner: 0,
          rolls: [
            [3, 3],
            [6, 2],
          ],
        },
      };
      return route.fulfill({ json: state });
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    const board = page.locator('.play-board svg.board');
    await expect(board.locator('[data-opening-die="pending"]')).toHaveCount(2);
    await expect(page.locator('.play-opening [data-opening-die]')).toHaveCount(
      0,
    );
    await page
      .getByRole('button', { name: 'Roll one die each', exact: true })
      .click();
    await expect(board.locator('.play-die-rolling')).toHaveCount(2);
    await expect(page.locator('.play-opening-winner')).toBeVisible();
    await expect(
      board.locator('[data-opening-player="0"] [data-opening-die]'),
    ).toHaveAttribute('data-opening-die', '6');
    await expect(
      board.locator('[data-opening-player="1"] [data-opening-die]'),
    ).toHaveAttribute('data-opening-die', '2');
    const theme = archiveThemes.find((theme) => theme.id === config.theme);
    if (theme) {
      await expect(
        board.locator('[data-opening-player="0"] image'),
      ).toHaveAttribute('href', theme.dice.white[5].src);
      await expect(
        board.locator('[data-opening-player="1"] image'),
      ).toHaveAttribute('href', theme.dice.black[1].src);
    } else {
      await expect(
        board.locator('[data-opening-player="0"] .white-checker'),
      ).toHaveCount(1);
      await expect(
        board.locator('[data-opening-player="1"] .black-checker'),
      ).toHaveCount(1);
    }
    const bounds = await board.boundingBox(),
      white = await board.locator('[data-opening-player="0"]').boundingBox(),
      black = await board.locator('[data-opening-player="1"]').boundingBox();
    for (const die of [white, black]) {
      expect(die).not.toBeNull();
      expect(die!.x).toBeGreaterThan(bounds!.x);
      expect(die!.x + die!.width).toBeLessThan(bounds!.x + bounds!.width);
      expect(die!.y).toBeGreaterThan(bounds!.y);
      expect(die!.y + die!.height).toBeLessThan(bounds!.y + bounds!.height);
    }
    expect(
      config.direction === 'right' ? white!.x > black!.x : white!.x < black!.x,
    ).toBe(true);
    await expect(page.locator('.play-opening-ties li')).toHaveCount(1);
    await page.screenshot({
      path: test.info().outputPath('opening-on-board.png'),
      fullPage: true,
    });
    await page.reload();
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await expect(
      board.locator('[data-opening-player="0"] [data-opening-die]'),
    ).toHaveAttribute('data-opening-die', '6');
    await expect(board.locator('.play-die-rolling')).toHaveCount(0);
    expect(writes).toBe(1);
  });
