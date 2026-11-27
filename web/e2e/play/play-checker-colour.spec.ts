import { gotoWorkspace } from '../helpers/navigation';
import { fulfillJsonList, fulfillJson } from '../helpers/api-fixtures';
import { installedBoardThemes } from '../helpers/board-themes';
import { playSessionFixture } from '../helpers/play-session-fixture';
import type { Page } from '@playwright/test';
import { eitherOrder } from '../helpers/play-turn-fixtures';
import { test, expect } from '../helpers/test';

const id = 'e0c7f6d0-58a1-4f43-9d39-2b8f1f0c6a11';

const session = (human: 0 | 1) =>
  playSessionFixture({
    id,
    human_player: human,
    engine: {
      id: 'gnubg',
      version: 'GNU Backgammon 1.08.003 native',
      plies: 3,
    },
    legal_turns: eitherOrder([3, 1], '8/5 6/5', human),
    legal_moves: [],
  });

// Mean luminance of the checker artwork rendered on each point: the fill of a
// plain SVG checker, or the opaque pixels of a textured checker image. The
// stacks are read in one synchronous step, so a re-render cannot detach them.
async function stackLuminance(page: Page, points: number[]) {
  return page.evaluate(async (points) => {
    const luminance = (r: number, g: number, b: number) =>
      0.2126 * r + 0.7152 * g + 0.0722 * b;
    const sources = points.map((point) => {
      const stack = document.querySelector(
        `.play-board svg.board g[data-point="${point}"]`,
      )!;
      const image = stack.querySelector('image');
      if (image) return image.getAttribute('href')!;
      const [r, g, b] = getComputedStyle(stack.querySelector('circle')!)
        .fill.match(/\d+/g)!
        .map(Number);
      return luminance(r, g, b);
    });
    return Promise.all(
      sources.map(async (source) => {
        if (typeof source === 'number') return source;
        const bitmap = new Image();
        bitmap.src = source;
        await bitmap.decode();
        const canvas = document.createElement('canvas');
        canvas.width = bitmap.naturalWidth;
        canvas.height = bitmap.naturalHeight;
        const context = canvas.getContext('2d')!;
        context.drawImage(bitmap, 0, 0);
        const { data } = context.getImageData(
          0,
          0,
          canvas.width,
          canvas.height,
        );
        let total = 0;
        let count = 0;
        for (let i = 0; i < data.length; i += 4)
          if (data[i + 3] >= 128) {
            total += luminance(data[i], data[i + 1], data[i + 2]);
            count += 1;
          }
        return total / count;
      }),
    );
  }, points);
}

async function startMatch(page: Page, theme: string, color: 'White' | 'Black') {
  const human = color === 'White' ? 0 : 1;
  const requests: unknown[] = [];
  await page.addInitScript(
    (theme) => localStorage.setItem('neondojo.theme', theme),
    theme,
  );
  await page.route('**/api/play/sessions', (route) => {
    if (route.request().method() !== 'POST')
      return fulfillJsonList<'PlaySession'>(route, []);
    requests.push(route.request().postDataJSON());
    return fulfillJson<'PlaySession'>(route, session(human), 201);
  });
  await gotoWorkspace(page, 'play');
  await page.getByRole('radio', { name: color, exact: true }).check();
  await page.getByRole('button', { name: 'Start match', exact: true }).click();
  await expect(page.locator('.play-board')).toHaveAttribute(
    'data-human-player',
    String(human),
  );
  expect(requests).toEqual([expect.objectContaining({ human_player: human })]);
  await expect(page.locator('.play-board .board-shell')).toHaveClass(
    new RegExp(
      `\\btheme-${theme.startsWith('bgblitz:') ? 'bgblitz' : theme}\\b`,
    ),
  );
  // Point 6 holds player 0's checkers, point 1 player 1's.
  const [player0, player1] = await stackLuminance(page, [6, 1]);
  return human === 0
    ? { human: player0, opponent: player1 }
    : { human: player1, opponent: player0 };
}

for (const { id: theme } of installedBoardThemes)
  test(`White plays the light checkers on ${theme}`, async ({ page }) => {
    const { human, opponent } = await startMatch(page, theme, 'White');
    expect(human).toBeGreaterThan(opponent);
  });

test('Black plays the dark checkers on BG Moon', async ({ page }) => {
  test.skip(
    !installedBoardThemes.some(({ id }) => id === 'bgblitz:bg-moon'),
    'BG Moon archive not present',
  );
  const { human, opponent } = await startMatch(
    page,
    'bgblitz:bg-moon',
    'Black',
  );
  expect(human).toBeLessThan(opponent);
});
