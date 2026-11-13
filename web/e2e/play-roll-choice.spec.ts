import { test, expect } from './test';
import type { PlaySession } from '../src/play-api';
const id = 'e10e2afb-bde6-486e-a041-ae982f6d1363';
for (const config of [
  {
    human: 0,
    direction: 'right',
    theme: 'forest',
    width: 1440,
    action: 'roll',
    index: 0,
    keyboard: false,
    canDouble: true,
  },
  {
    human: 1,
    direction: 'left',
    theme: 'bgblitz:bg-earth',
    width: 1440,
    action: 'roll',
    index: 1,
    keyboard: true,
    canDouble: true,
  },
  {
    human: 0,
    direction: 'left',
    theme: 'forest',
    width: 390,
    action: 'double',
    index: 0,
    keyboard: false,
    canDouble: true,
  },
  {
    human: 1,
    direction: 'right',
    theme: 'bgblitz:bg-earth',
    width: 1440,
    action: 'double',
    index: 0,
    keyboard: true,
    canDouble: true,
  },
  {
    human: 0,
    direction: 'right',
    theme: 'forest',
    width: 390,
    action: 'roll',
    index: 1,
    keyboard: false,
    canDouble: false,
  },
])
  test(`board ${config.action}/${config.human}/${config.direction}/${config.width}/${config.keyboard}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: config.width, height: 1100 });
    let saved: PlaySession = {
      id,
      version: 0,
      human_player: config.human,
      human_name: 'Player',
      match_length: 7,
      bearoff_direction: config.direction as 'left' | 'right',
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
      phase: 'roll',
      can_double: config.canDouble,
      legal_moves: [],
      history: [],
      scores: [0, 0],
      game_no: 1,
      winner: null,
      opening: null,
    };
    const writes: unknown[] = [];
    let release = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    await page.addInitScript(
      ({ id, theme }) => {
        localStorage.setItem('bgtutor.locale', 'en');
        localStorage.setItem('bgtutor.play-session', id);
        localStorage.setItem('bgtutor.theme', theme);
      },
      { id, theme: config.theme },
    );
    await page.route(`**/api/play/sessions/${id}`, (route) =>
      route.fulfill({ json: saved }),
    );
    await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
      writes.push(route.request().postDataJSON());
      await gate;
      saved = {
        ...saved,
        version: 1,
        phase: 'move',
        can_double: false,
        position: { ...saved.position, dice: [3, 1] },
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
      await route.fulfill({ json: saved });
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    const dice = page.locator('[data-dice-action="roll"]');
    const cube = page.locator('.play-board [data-cube]');
    try {
      await expect(dice).toHaveCount(2);
      const toolbar = page.getByTestId('play-board-toolbar');
      await expect(toolbar).toBeVisible();
      await expect(page.getByRole('alertdialog')).toHaveCount(0);
      await expect(cube).toHaveAttribute(
        'aria-disabled',
        String(!config.canDouble),
      );
      await expect(toolbar.locator('[data-play-action="double"]')).toHaveCount(
        1,
      );
      if (config.canDouble)
        await expect(
          toolbar.getByRole('button', { name: 'Double', exact: true }),
        ).toBeEnabled();
      else {
        const double = toolbar.getByRole('button', {
          name: 'Double',
          exact: true,
        });
        await expect(double).toBeDisabled();
        await expect(toolbar.locator('.play-double-unavailable')).toBeVisible();
        await double.dispatchEvent('click');
        expect(writes).toEqual([]);
      }
      const target = toolbar.locator(`[data-play-action="${config.action}"]`);
      if (config.keyboard) {
        await target.focus();
        await page.keyboard.press(config.action === 'roll' ? 'Enter' : ' ');
      } else await target.click();
      await expect.poll(() => writes.length).toBe(1);
      expect(writes).toEqual([{ version: 0, action: config.action }]);
      await expect(page.locator('.play-workspace')).toHaveAttribute(
        'aria-busy',
        'true',
      );
      await expect(page.locator('[data-dice-action="roll"]')).toHaveCount(0);
      await expect(cube).toHaveAttribute('aria-disabled', 'true');
      release();
      await expect(page.locator('.play-workspace')).toHaveAttribute(
        'aria-busy',
        'false',
      );
      expect(writes).toHaveLength(1);
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth),
      ).toBeLessThanOrEqual(config.width);
    } finally {
      release();
    }
  });

for (const reason of [
  {
    name: 'Crawford',
    exponent: 0,
    owner: 3,
    crawford: true,
    text: 'Doubling is unavailable in the Crawford game.',
  },
  {
    name: 'opponent owns cube',
    exponent: 0,
    owner: 1,
    crawford: false,
    text: 'Your opponent owns the cube; only they can redouble.',
  },
  {
    name: 'cube covers match points',
    exponent: 3,
    owner: 0,
    crawford: false,
    text: 'No further double is offered: the current cube already covers the points you need to win the match.',
  },
])
  test(`disabled Double explains ${reason.name}`, async ({ page }) => {
    const { historyFixture } = await import('./play-history-review-helpers');
    const { saved, writes } = await historyFixture(page);
    Object.assign(saved, {
      phase: 'roll',
      can_double: false,
      scores: [2, 1],
      play_mode: 'competition',
    });
    const bytes = Buffer.from(saved.position.gnubg_match_id, 'base64');
    bytes[0] =
      (bytes[0] & 64) |
      reason.exponent |
      (reason.owner << 4) |
      (reason.crawford ? 128 : 0);
    saved.position.gnubg_match_id = bytes.toString('base64');
    await page.reload();
    await page.locator('#workspace-tab-play').click();
    const toolbar = page.getByTestId('play-board-toolbar');
    const double = toolbar.getByRole('button', { name: 'Double', exact: true });
    await expect(double).toBeVisible();
    await expect(double).toBeDisabled();
    await expect(double).toHaveAccessibleDescription(reason.text);
    await expect(toolbar.locator('.play-double-unavailable')).toHaveText(
      reason.text,
    );
    expect(writes).toEqual([]);
  });
