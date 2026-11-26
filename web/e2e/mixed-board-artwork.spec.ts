import { installedBoardThemes } from './board-themes';
import { historyFixture } from './play-history-review-helpers';
import { applyAndCloseSettings } from './settings-actions';
import { backendTest, expect, type Locator } from './test';

async function expectDiceArtwork(board: Locator, theme: string) {
  const die = board.locator('[data-die-owner]').first();
  if (theme.startsWith('bgblitz:'))
    await expect(die.locator('image')).toHaveAttribute(
      'href',
      new RegExp(`/themes/${theme.slice('bgblitz:'.length)}/dice_`),
    );
  else {
    await expect(die.locator('svg')).toHaveAttribute('data-dice-theme', theme);
    await expect(die.locator('circle')).toHaveCount(3);
  }
}

backendTest(
  'saved mixed artwork fits the playing board with five edge-to-edge checkers',
  async ({ page }) => {
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
      await dialog
        .getByRole('combobox', { name: 'Checker set' })
        .selectOption(checkers);
      await dialog
        .getByRole('combobox', { name: 'Doubling cube' })
        .selectOption(cube);
      await dialog
        .getByRole('combobox', { name: 'Dice', exact: true })
        .selectOption(dice ?? '');
      await expectDiceArtwork(
        dialog.locator('.settings-preview'),
        expectedDice,
      );
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
      await expectDiceArtwork(playing, expectedDice);
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
