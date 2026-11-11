import { test, expect } from './test';
const id = '8c390ba7-76a1-44f0-a041-ae982f6d1363';
for (const config of [
  { human: 0, direction: 'right', theme: 'forest', width: 1440 },
  { human: 1, direction: 'left', theme: 'bgblitz:bg-earth', width: 1440 },
  { human: 0, direction: 'right', theme: 'forest', width: 390 },
  { human: 0, direction: 'right', theme: 'forest', width: 800 },
])
  test(`analysis-style playing history ${config.human}/${config.direction}/${config.width}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: config.width, height: 1100 });
    const history = Array.from({ length: 80 }, (_, i) => ({
      player: i % 2,
      dice: i === 2 || i === 3 ? [] : i % 2 ? [4, 2] : [3, 1],
      notation:
        i === 2 ? 'double' : i === 3 ? 'take' : i % 2 ? '8/4 6/4' : '8/5 6/5',
    }));
    const saved = {
      id,
      version: 40,
      human_player: config.human,
      human_name: 'Human player',
      match_length: 7,
      bearoff_direction: config.direction,
      engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 2 },
      position: {
        id,
        match_uuid: id,
        ordinal: 40,
        game_no: 2,
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
      can_double: true,
      legal_moves: [],
      history,
      scores: [1, 0],
      game_no: 2,
      winner: null,
      opening: null,
    };
    const writes: unknown[] = [];
    await page.addInitScript(
      ({ id, theme }) => {
        localStorage.setItem('bgtutor.locale', 'en');
        localStorage.setItem('bgtutor.play-session', id);
        localStorage.setItem('bgtutor.theme', theme);
      },
      { id, theme: config.theme },
    );
    await page.route(`**/api/play/sessions/${id}`, (r) =>
      r.fulfill({ json: saved }),
    );
    await page.route(`**/api/play/sessions/${id}/actions`, (r) => {
      writes.push(r.request().postDataJSON());
      return r.fulfill({ json: saved });
    });
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    const panel = page.locator('.play-history'),
      rows = panel.locator('.move-history-row');
    await expect(rows).toHaveCount(80);
    await expect(rows.first().locator('.move-no')).toHaveText('1');
    await expect(
      rows.first().locator('.move-history-details > strong'),
    ).toHaveText('8/5 6/5');
    await expect(rows.first().locator('.move-dice')).toHaveText('3–1');
    await expect(rows.first().locator('.player-dot')).toHaveClass(/white/);
    await expect(rows.nth(1).locator('.player-dot')).toHaveClass(/black/);
    await expect(rows.nth(2).locator('.move-dice')).toHaveCount(0);
    await expect(rows.last()).toHaveAttribute('aria-current', 'step');
    await expect(panel.locator('.section-heading')).toContainText('80');
    const list = panel.locator('.moves-list');
    await expect
      .poll(() => list.evaluate((el) => el.scrollTop))
      .toBeGreaterThan(0);
    const box = await panel.boundingBox(),
      board = await page.locator('.play-board').boundingBox();
    if (config.width > 900) {
      expect(box!.x + box!.width).toBeLessThanOrEqual(board!.x);
      expect(box!.width).toBeLessThanOrEqual(280);
      expect(board!.width).toBeGreaterThan(box!.width * 2);
      expect(
        await page.evaluate(() => document.documentElement.scrollHeight),
      ).toBeLessThan(2200);
    } else expect(box!.y).toBeGreaterThan(board!.y);
    await expect(list).toHaveCSS('display', 'block');
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(config.width);
    await expect(rows.first().locator('button')).toHaveCount(0);
    expect(writes).toEqual([]);
    await expect(
      panel.getByRole('link', { name: 'Download SGF', exact: true }),
    ).toHaveAttribute('href', `/api/play/sessions/${id}/sgf`);
    await page.screenshot({
      path: test.info().outputPath('playing-history.png'),
      fullPage: true,
    });
  });
