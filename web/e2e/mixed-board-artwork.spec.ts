import { installedBoardThemes } from './board-themes';
import { historyFixture } from './play-history-review-helpers';
import { applyAndCloseSettings } from './settings-actions';
import { backendTest, expect, type Locator } from './test';

async function expectDiceArtwork(die: Locator, theme: string, pips: number) {
  if (theme.startsWith('bgblitz:'))
    await expect(die.locator('image')).toHaveAttribute(
      'href',
      new RegExp(`/themes/${theme.slice('bgblitz:'.length)}/dice_`),
    );
  else {
    await expect(die.locator('svg')).toHaveAttribute('data-dice-theme', theme);
    await expect(die.locator('circle')).toHaveCount(pips);
  }
}

backendTest(
  'saved mixed artwork fits the playing board with five edge-to-edge checkers',
  async ({ page }) => {
    await page.setViewportSize({ width: 1800, height: 1169 });
    // Resume a position with dice on the table. Configuration uses the real backend.
    await historyFixture(page);
    const moon = installedBoardThemes.find(
      ({ id }) => id === 'bgblitz:bg-moon',
    );
    const poseidon = installedBoardThemes.find(
      ({ id }) => id === 'bgblitz:bg-poseidon',
    );
    const cases = [
      {
        board: 'Terra Rossa',
        checkers: 'ion',
        cube: 'slate',
        dice: 'match-board',
        expectedDice: 'terra-rossa',
        piece: /ion-white-/,
        cubeImage: false,
      },
      {
        board: 'Slate',
        checkers: 'ion',
        cube: 'forest',
        dice: null,
        expectedDice: 'ion',
        piece: /ion-white-/,
        cubeImage: false,
      },
      ...(moon && poseidon
        ? [
            {
              board: moon.name,
              checkers: 'pelagic',
              cube: poseidon.id,
              dice: poseidon.id,
              expectedDice: poseidon.id,
              piece: /pelagic-white-/,
              cubeImage: true,
            },
            {
              board: 'Moonstone',
              checkers: moon.id,
              cube: 'carbon',
              dice: 'forest',
              expectedDice: 'forest',
              piece: /\/themes\/bg-moon\//,
              cubeImage: false,
            },
            {
              board: moon.name,
              checkers: 'walnut',
              cube: poseidon.id,
              dice: 'match-board',
              expectedDice: moon.id,
              piece: null,
              cubeImage: true,
            },
          ]
        : []),
    ];
    for (const {
      board,
      checkers,
      cube,
      dice,
      expectedDice,
      piece,
      cubeImage,
    } of cases) {
      await page.goto('/#home');
      await page.getByRole('button', { name: 'Settings', exact: true }).click();
      const dialog = page.getByRole('dialog', {
        name: 'Settings',
        exact: true,
      });
      await dialog.getByRole('button', { name: 'Board', exact: true }).click();
      await dialog.getByRole('button', { name: board, exact: true }).click();
      const checkerChoice = dialog.getByRole('combobox', {
        name: 'Checker set',
      });
      const cubeChoice = dialog.getByRole('combobox', {
        name: 'Doubling cube',
      });
      await dialog
        .getByRole('combobox', { name: 'Dice', exact: true })
        .selectOption(dice ?? '');
      const preview = dialog.locator('.settings-preview');
      const light = preview.locator('[data-die-owner="0"]');
      const dark = preview.locator('[data-die-owner="1"]');
      if (expectedDice === 'terra-rossa') {
        await checkerChoice.selectOption('');
        await cubeChoice.selectOption('');
        await expect(dark).toHaveCount(1);
        await expect(light.locator('stop[offset=".6"]')).toHaveAttribute(
          'stop-color',
          '#fff1cf',
        );
        await expect(dark.locator('stop[offset=".6"]')).toHaveAttribute(
          'stop-color',
          '#ba101c',
        );
        for (const [color, count] of [
          ['white', '1'],
          ['black', '-1'],
        ]) {
          const tray = preview.locator(`[data-point="off-${color}"]`);
          await expect(tray).toHaveAttribute('data-count', count);
          const checker = tray.locator('image');
          await expect(checker).toHaveAttribute(
            'href',
            new RegExp(`terra-rossa-${color}-`),
          );
          const fitsBoard = await checker.evaluate((piece: SVGImageElement) => {
            const { x, width } = piece.getBBox();
            const view = piece.ownerSVGElement!.viewBox.baseVal;
            return x >= view.x && x + width <= view.x + view.width;
          });
          expect(fitsBoard).toBe(true);
        }
        await preview.screenshot({
          path: backendTest.info().outputPath('terra-rossa-preview.png'),
        });
      }
      await checkerChoice.selectOption(checkers);
      await cubeChoice.selectOption(cube);
      await expectDiceArtwork(light, expectedDice, 3);
      await expectDiceArtwork(dark, expectedDice, 2);
      await applyAndCloseSettings(dialog);
      const state = await (await page.request.get('/api/configuration')).json();
      expect(state.current_configuration.appearance).toMatchObject({
        board_theme: installedBoardThemes.find(({ name }) => name === board)!
          .id,
        checker_theme: checkers,
        cube_theme: cube,
        dice_theme: dice,
      });
      await page.reload();
      await page.getByRole('tab', { name: 'Playing', exact: true }).click();
      const playing = page.locator('.play-live-board > .board-shell');
      await expect(playing).toHaveAttribute('data-checker-theme', checkers);
      await expectDiceArtwork(
        playing.locator('[data-die-owner]').first(),
        expectedDice,
        3,
      );
      const stack = playing.locator('[data-point="13"]');
      const discs = stack.locator(piece ? 'image' : 'circle.white-checker');
      await expect(discs).toHaveCount(5);
      if (piece) await expect(discs.first()).toHaveAttribute('href', piece);
      else await expect(stack.locator('.theme-walnut')).toHaveCount(5);
      const gaps = await discs.evaluateAll((nodes) => {
        const boxes = nodes
          .map((node) => {
            const { top, bottom } = node.getBoundingClientRect();
            // getBoundingClientRect excludes the circle's painted outline.
            const stroke =
              node instanceof SVGCircleElement
                ? parseFloat(getComputedStyle(node).strokeWidth) *
                  Math.abs(node.getScreenCTM()!.d)
                : 0;
            return { top: top - stroke / 2, bottom: bottom + stroke / 2 };
          })
          .sort((a, b) => a.top - b.top);
        return boxes.slice(1).map((box, i) => box.top - boxes[i].bottom);
      });
      for (const gap of gaps) expect(Math.abs(gap)).toBeLessThan(0.01);
      const cubeArt = playing.locator('.play-cube-artwork');
      await expect(cubeArt).toHaveAttribute('data-cube-theme', cube);
      if (cubeImage)
        await expect(cubeArt.locator('image')).toHaveAttribute(
          'href',
          /\/themes\/bg-poseidon\/cube/,
        );
      else await expect(cubeArt.locator('svg')).toHaveClass(`theme-${cube}`);
    }
  },
);
