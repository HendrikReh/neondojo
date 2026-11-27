import { fulfillPlaySession } from '../helpers/api-fixtures';
import { eitherOrder } from '../helpers/play-turn-fixtures';
import {
  mockPlaySession,
  playDecisionFixture,
  playSessionFixture,
} from '../helpers/play-session-fixture';
import { test, expect } from '../helpers/test';
import { botDice } from '../helpers/play-board-actions';
const id = 'e10e2afb-bde6-486e-a041-ae982f6d1363';
for (const config of [
  {
    human: 0,
    direction: 'right' as const,
    theme: 'forest',
    width: 1800,
    action: 'roll',
    keyboard: false,
    canDouble: true,
  },
  {
    human: 1,
    direction: 'left' as const,
    theme: 'bgblitz:bg-earth',
    width: 2560,
    action: 'roll',
    keyboard: true,
    canDouble: true,
  },
  {
    human: 0,
    direction: 'left' as const,
    theme: 'forest',
    width: 390,
    action: 'double',
    keyboard: false,
    canDouble: true,
  },
  {
    human: 1,
    direction: 'right' as const,
    theme: 'bgblitz:bg-earth',
    width: 3840,
    action: 'double',
    keyboard: true,
    canDouble: true,
  },
  {
    human: 0,
    direction: 'right' as const,
    theme: 'forest',
    width: 390,
    action: 'roll',
    keyboard: false,
    canDouble: false,
  },
])
  test(`board ${config.action}/${config.human}/${config.direction}/${config.width}/${config.keyboard}`, async ({
    page,
  }) => {
    await page.setViewportSize({
      width: config.width,
      height:
        ({ 1800: 1169, 2560: 1440, 3840: 2160 } as Record<number, number>)[
          config.width
        ] ?? 900,
    });
    let release = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const { writes } = await mockPlaySession(page, {
      session: playSessionFixture({
        id,
        human_player: config.human,
        bearoff_direction: config.direction,
        phase: 'roll',
        can_double: config.canDouble,
        history: [
          { player: 1 - config.human, dice: [6, 2], notation: '13/7 6/4' },
        ],
      }),
      mockCollection: false,
      locale: 'en',
      theme: config.theme,
      open: true,
      onAction: async (_body, saved) => {
        await gate;
        return {
          ...saved,
          version: 1,
          phase: 'move',
          can_double: false,
          position: { ...saved.position, dice: [3, 1] },
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
      },
    });
    const cube = page.locator('.play-board [data-cube]');
    try {
      const dice = page.locator(
        '.play-board [data-dice="6-2"] [data-play-die-index]',
      );
      await expect(dice).toHaveCount(2);
      await expect(dice.first()).toHaveAttribute('data-used', 'true');
      await expect(dice.nth(1)).toHaveAttribute('data-used', 'true');
      await expect(dice.first().locator('.play-die-artwork')).toHaveCSS(
        'opacity',
        '0.28',
      );
      await expect(
        page.locator(`[data-die-owner="${1 - config.human}"]`),
      ).toHaveCount(2);
      await expect(page.locator('.play-board [data-dice="6-2"]')).toBeVisible();
      await expect(page.locator('dialog:modal')).toHaveCount(0);
      await expect(
        page.getByRole('button', { name: 'Next roll', exact: true }),
      ).toHaveCount(0);
      await expect(cube).toHaveAttribute(
        'aria-disabled',
        String(!config.canDouble),
      );
      const artwork = dice.locator('.play-die-artwork');
      for (const die of await dice.all()) {
        await die.hover();
        await expect(artwork.first()).toHaveCSS('opacity', '0.55');
        await expect(artwork.last()).toHaveCSS('opacity', '0.55');
      }
      async function diceGap() {
        const first = (await artwork.first().boundingBox())!;
        const last = (await artwork.last().boundingBox())!;
        const [left, right] = [first, last].sort((a, b) => a.x - b.x);
        expect(left.x + left.width).toBeLessThan(right.x);
        return {
          x: (left.x + left.width + right.x) / 2,
          y: left.y + left.height / 2,
        };
      }
      const gap = await diceGap();
      await page.mouse.move(gap.x, gap.y);
      await expect(artwork.first()).toHaveCSS('opacity', '0.55');
      await expect(artwork.last()).toHaveCSS('opacity', '0.55');
      await expect(botDice(page)).toHaveCount(1);
      await expect(botDice(page)).toHaveCSS('outline-style', 'none');
      for (const face of await artwork.all()) {
        await expect(face).toHaveCSS('filter', /drop-shadow\(/);
        await expect(face).toHaveCSS('transform', /matrix\(/);
      }
      await expect(botDice(page)).toHaveRole('button');
      await expect(botDice(page)).toHaveAttribute('tabindex', '0');
      const faces = await artwork.all();
      const liftedDice = await Promise.all(
        faces.map((face) => face.boundingBox()),
      );
      await page.mouse.down();
      for (const [index, face] of faces.entries()) {
        await expect
          .poll(async () => (await face.boundingBox())!.y)
          .toBeGreaterThan(liftedDice[index]!.y + 0.5);
      }
      await page.mouse.move(0, 0);
      await page.mouse.up();
      expect(writes).toEqual([]);
      const cubeArt = cube.locator('image, rect:not([aria-hidden])').first();
      const restingCube = (await cubeArt.boundingBox())!;
      if (config.canDouble) {
        await cube.hover();
        await expect(cube).toHaveCSS('outline-style', 'none');
        await expect
          .poll(async () => (await cubeArt.boundingBox())!.y)
          .toBeLessThan(restingCube.y - 0.5);
        await expect(cube.locator('.play-cube-artwork')).toHaveCSS(
          'filter',
          /drop-shadow\(/,
        );
        const liftedCube = (await cubeArt.boundingBox())!;
        await page.mouse.down();
        await expect
          .poll(async () => (await cubeArt.boundingBox())!.y)
          .toBeGreaterThan(liftedCube.y + 0.5);
        await page.mouse.move(0, 0);
        await page.mouse.up();
        expect(writes).toEqual([]);
        await cube.hover();
        await page.screenshot({
          path: test.info().outputPath('cube-hover.png'),
        });
      } else {
        const bounds = (await cube.boundingBox())!;
        await page.mouse.move(
          bounds.x + bounds.width / 2,
          bounds.y + bounds.height / 2,
        );
        await expect(cube).toHaveCSS('outline-style', 'none');
        await expect(cube.locator('.play-cube-artwork')).toHaveCSS(
          'filter',
          'none',
        );
        expect((await cubeArt.boundingBox())!.y).toBeCloseTo(restingCube.y, 1);
      }
      await expect(artwork.first()).toHaveCSS('opacity', '0.28');
      await expect(artwork.last()).toHaveCSS('opacity', '0.28');
      for (const face of await artwork.all()) {
        await expect(face).toHaveCSS('transform', 'none');
      }
      expect(writes).toEqual([]);
      const target = config.action === 'roll' ? botDice(page) : cube;
      if (config.action === 'roll') {
        await target.scrollIntoViewIfNeeded();
        const gap = await diceGap();
        await page.mouse.move(gap.x, gap.y);
      } else await target.hover();
      await page.screenshot({
        path: test.info().outputPath('roll-choice.png'),
      });
      if (config.keyboard) {
        await target.focus();
        await page.keyboard.press(config.action === 'roll' ? 'Enter' : ' ');
      } else if (config.action === 'roll') {
        const gap = await diceGap();
        await page.mouse.click(gap.x, gap.y);
      } else await target.click();
      await expect.poll(() => writes.length).toBe(1);
      expect(writes).toEqual([{ version: 0, action: config.action }]);
      await expect(page.locator('.play-workspace')).toHaveAttribute(
        'aria-busy',
        'true',
      );
      await expect(page.locator('dialog:modal')).toHaveCount(0);
      await expect(cube).toHaveCSS('pointer-events', 'none');
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
    const { historyFixture } =
      await import('../helpers/play-history-review-helpers');
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
    const cube = page.locator('.play-board [data-cube]');
    await expect(cube).toHaveAttribute('aria-disabled', 'true');
    await cube.dispatchEvent('click');
    await expect(cube).toHaveAccessibleDescription(reason.text);
    await expect(
      page
        .getByTestId('play-board-toolbar')
        .locator('.play-double-unavailable'),
    ).toHaveText(reason.text);
    expect(writes).toEqual([]);
  });

for (const action of ['roll', 'double'] as const) {
  test(`Tutor can intercept a direct board ${action} before it is played`, async ({
    page,
  }) => {
    const { historyFixture, historySessionId } =
      await import('../helpers/play-history-review-helpers');
    const { saved, writes } = await historyFixture(page);
    Object.assign(saved, {
      phase: 'roll',
      play_mode: 'tutor',
      can_double: true,
    });
    saved.position.dice = [];
    const decisionId = '7216ea44-6ad0-4e8c-a543-f809a63de1d1';
    await page.route(
      `**/api/play/sessions/${historySessionId}/actions`,
      (route) => {
        const choice = route.request().postDataJSON();
        writes.push(choice);
        if (choice.decision_id) {
          Object.assign(saved, {
            version: 9,
            phase: 'move',
            can_double: false,
            pending_decision: null,
            position: { ...saved.position, dice: [5, 2] },
          });
        } else {
          Object.assign(saved, {
            version: 8,
            pending_decision: playDecisionFixture({
              id: decisionId,
              action,
              original_action: action,
              move_index: null,
              original_move_index: null,
              warning: true,
              revealed: false,
              loss: null,
              first_loss: null,
              severity: null,
              first_severity: null,
            }),
          });
        }
        return fulfillPlaySession(route, saved);
      },
    );
    await page.reload();
    await page.locator('#workspace-tab-play').click();
    const cube = page.locator('.play-board [data-cube]');
    await (action === 'roll' ? botDice(page).last() : cube).click();
    const warning = page.getByRole('alertdialog', {
      name: 'Tutor',
      exact: true,
    });
    await expect(warning).toBeVisible();
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-phase',
      'roll',
    );
    await expect(page.locator('.play-die-rolling')).toHaveCount(0);
    await expect(botDice(page)).toHaveCount(0);
    await expect(cube).toHaveCSS('pointer-events', 'none');
    expect(writes).toEqual([{ version: 7, action }]);
    await warning
      .getByRole('button', { name: 'Play anyway', exact: true })
      .click();
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-phase',
      'move',
    );
    expect(writes).toEqual([
      { version: 7, action },
      { version: 8, action, decision_id: decisionId },
    ]);
  });
}
