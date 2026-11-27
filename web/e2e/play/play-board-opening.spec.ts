import { archiveBoardThemes as archiveThemes } from '../helpers/board-themes';
import {
  mockPlaySession,
  playSessionFixture,
} from '../helpers/play-session-fixture';
import { test, expect } from '../helpers/test';
const id = 'ec9b317a-615f-49b1-b1de-8cddaf2561f1';
for (const config of [
  { human: 0, direction: 'right' as const, theme: 'forest' },
  { human: 1, direction: 'left' as const, theme: 'forest' },
  { human: 0, direction: 'right' as const, theme: 'bgblitz:bg-earth' },
  { human: 1, direction: 'left' as const, theme: 'bgblitz:bg-earth' },
  { human: 0, direction: 'left' as const, theme: 'bgblitz:bg-earth' },
  { human: 1, direction: 'right' as const, theme: 'bgblitz:bg-earth' },
])
  test(`opening rolls native-themed white and black dice inside board ${config.human}/${config.direction}/${config.theme}`, async ({
    page,
  }) => {
    let finishOpening = () => {};
    const continued = new Promise<void>((resolve) => (finishOpening = resolve));
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await mockPlaySession(page, {
      session: playSessionFixture({
        id,
        human_player: config.human,
        bearoff_direction: config.direction,
        phase: 'opening_roll',
      }),
      theme: config.theme,
      open: true,
      onAction: async (body, state) => {
        if (body.action === 'start_opening_move') {
          await continued;
          return {
            ...state,
            version: 2,
            phase: 'move',
            position: { ...state.position, player: config.human },
          };
        }
        return {
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
      },
    });
    const board = page.locator('.play-board svg.board');
    await expect(board.locator('.play-die-rolling')).toHaveCount(2);
    await expect(page.locator('dialog:modal')).toHaveCount(0);
    await expect(
      board.locator('[data-opening-die="6"] [data-rolling="false"]'),
    ).toBeVisible();
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
        board.locator('[data-opening-player="0"] circle'),
      ).toHaveCount(6);
      await expect(
        board.locator('[data-opening-player="1"] circle'),
      ).toHaveCount(2);
    }
    // The winning roll belongs to White, including when the human plays Black.
    const numberOne = theme
      ? board
          .locator('[data-number-orientation] image')
          .nth(config.human === 0 ? 1 : 0)
      : board.locator('.point-number').filter({ hasText: /^1$/ });
    await expect(numberOne).toBeVisible();
    if (theme) {
      const orientation = `${config.direction === 'left' ? 'LR' : 'RL'}${config.human === 0 ? 'D' : 'U'}`;
      await expect(numberOne).toHaveAttribute(
        'href',
        theme.numbers[orientation][config.human === 0 ? 1 : 0].src,
      );
      if (theme.id === 'bgblitz:bg-earth' && config.direction === 'left') {
        // In both original BG Earth left-bearing strips, the 1 is centered
        // 46 pixels from the left edge of the 1190-pixel image.
        const strip = (await numberOne.boundingBox())!;
        const point = config.human === 0 ? 1 : 24;
        const target = (await board
          .locator(`[data-play-point="${point}"] .play-point`)
          .boundingBox())!;
        expect(strip.x + (46 / 1190) * strip.width).toBeCloseTo(
          target.x + target.width / 2,
          0,
        );
      }
    } else {
      await expect(board.locator('.point-number')).toHaveCount(24);
      const target = board.locator(
        `[data-play-point="${config.human === 0 ? 1 : 24}"] .play-point`,
      );
      const labelBounds = (await numberOne.boundingBox())!;
      const targetBounds = (await target.boundingBox())!;
      expect(labelBounds.y + labelBounds.height / 2).toBeGreaterThan(
        targetBounds.y + targetBounds.height,
      );
      expect(labelBounds.x + labelBounds.width / 2).toBeCloseTo(
        targetBounds.x + targetBounds.width / 2,
        0,
      );
    }
    expect(
      await numberOne.evaluate((el) => {
        const matrix = (el as SVGGraphicsElement).getScreenCTM()!;
        return matrix.a > 0 && matrix.d > 0;
      }),
    ).toBe(true);
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
    await page.screenshot({
      path: test.info().outputPath('opening-on-board.png'),
      fullPage: true,
    });
    finishOpening();
    await expect(board.locator('[data-opening-die]')).toHaveCount(0);
  });
