import { installedBoardThemes } from './board-themes';
import { applyAndCloseSettings } from './settings-actions';
import { backendTest, expect } from './test';

backendTest(
  'saved mixed artwork fits the playing board with five edge-to-edge checkers',
  async ({ page }) => {
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
        piece: /ion-white-/,
        cubeImage: false,
      },
      ...(moon && poseidon
        ? [
            {
              board: moon.name,
              checkers: 'pelagic',
              cube: poseidon.id,
              piece: /pelagic-white-/,
              cubeImage: true,
            },
            {
              board: 'Moonstone',
              checkers: moon.id,
              cube: 'carbon',
              piece: /\/themes\/bg-moon\//,
              cubeImage: false,
            },
            {
              board: moon.name,
              checkers: 'walnut',
              cube: poseidon.id,
              piece: null,
              cubeImage: true,
            },
          ]
        : []),
    ];
    for (const { board, checkers, cube, piece, cubeImage } of cases) {
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
      await applyAndCloseSettings(dialog);
      const state = await (await page.request.get('/api/configuration')).json();
      expect(state.current_configuration.appearance).toMatchObject({
        board_theme: installedBoardThemes.find(({ name }) => name === board)!
          .id,
        checker_theme: checkers,
        cube_theme: cube,
      });
      await page.reload();
      await page.getByRole('tab', { name: 'Playing', exact: true }).click();
      const playing = page.locator('.play-board .board-shell').first();
      await expect(playing).toHaveAttribute('data-checker-theme', checkers);
      const stack = playing.locator('[data-point="6"]');
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
