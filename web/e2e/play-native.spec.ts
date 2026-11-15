import { rollDice } from './play-board-actions';
import { watchOpening } from './play-opening-helpers';
import { test, expect } from '@playwright/test';
import type { PlaySession } from '../src/play-api';
declare global {
  interface Window {
    __nativeAudio: number[];
  }
}
import { orderedLegalTurns } from '../src/play-state';
test.skip(
  !process.env.BGTUTOR_NATIVE_PLAY_TEST,
  'Requires disposable PostgreSQL/backend with actual GNUbg',
);
for (const config of [
  { human: 0, direction: 'right', plies: 2, theme: 'forest' },
  { human: 1, direction: 'left', plies: 0, theme: 'bgblitz:bg-earth' },
]) {
  test(`real native bot ${config.human}/${config.direction}/${config.plies}-ply`, async ({
    page,
  }) => {
    await page.addInitScript(() => {
      window.__nativeAudio = [];
      const start = AudioBufferSourceNode.prototype.start;
      AudioBufferSourceNode.prototype.start = function (...args) {
        if (this.buffer)
          window.__nativeAudio.push(
            Math.round(this.buffer.duration * 100) / 100,
          );
        return start.apply(this, args);
      };
    });
    // Exercise the persisted configuration used by native session creation.
    await page.goto('/');
    await expect(page.getByRole('button', { name: /^Profile:/ })).toBeVisible({
      timeout: 30_000,
    });
    const current = await page.request.get('/api/configuration');
    expect(current.status()).toBe(200);
    const profile = await current.json();
    const configuration = profile.current_configuration;
    configuration.play.plies = config.plies;
    configuration.appearance.bearoff_direction = config.direction;
    configuration.appearance.board_theme = config.theme;
    configuration.general.play_sounds = true;
    const saved = await page.request.put('/api/configuration', {
      data: { expected_revision: profile.revision, configuration },
    });
    expect(saved.status(), await saved.text()).toBe(200);
    await page.reload();
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await page.getByLabel('Your checkers').selectOption(String(config.human));
    await page
      .getByRole('tabpanel', { name: 'Playing', exact: true })
      .getByLabel('Play mode', { exact: true })
      .selectOption('competition');
    const opening = watchOpening(page);
    const create = page.waitForResponse(
      (response) =>
        response.url().endsWith('/api/play/sessions') &&
        response.request().method() === 'POST',
    );
    await page
      .getByRole('button', { name: 'Start match', exact: true })
      .click();
    const response = await create;
    expect(response.status()).toBe(201);
    let state = await opening();
    expect(state.engine.version).toContain('1.08.003');
    expect(state.engine.plies).toBe(config.plies);
    expect(state.bearoff_direction).toBe(config.direction);
    expect(state.human_player).toBe(config.human);
    async function takeDouble() {
      const taken = page.waitForResponse(
        (r) =>
          r.url().endsWith('/actions') &&
          r.request().postDataJSON()?.action === 'take',
      );
      await page
        .getByRole('button', { name: 'Take double', exact: true })
        .click();
      const response = await taken;
      expect(response.status(), await response.text()).toBe(200);
      return (await response.json()) as PlaySession;
    }
    for (let turn = 0; turn < 2; turn++) {
      if (state.phase === 'double_response') state = await takeDouble();
      if (state.phase === 'roll') {
        await expect(rollDice(page)).toBeVisible();
        const rolling = page.waitForResponse(
          (r) =>
            r.url().endsWith('/actions') && r.request().method() === 'POST',
        );
        await rollDice(page).click();
        const roll = await rolling;
        expect(roll.status()).toBe(200);
        state = await roll.json();
      }
      expect(state.phase).toBe('move');
      expect(state.position.player).toBe(config.human);
      let moves = orderedLegalTurns(
        state.position.board,
        state.human_player,
        state.position.dice,
        state.legal_moves,
      );
      if (!moves.length && state.position.dice[0] !== state.position.dice[1]) {
        await page
          .getByRole('button', { name: /Swap dice order/ })
          .first()
          .click();
        moves = orderedLegalTurns(
          state.position.board,
          state.human_player,
          [...state.position.dice].reverse(),
          state.legal_moves,
        );
      }
      const move = moves[0];
      expect(move?.steps.length).toBeGreaterThan(0);
      for (const step of move.steps) {
        await expect(page.locator('.play-workspace')).toHaveAttribute(
          'aria-busy',
          'false',
        );
        const checker = page
          .locator(`[data-checker-hover-point="${step.from}"]`)
          .last();
        await checker.hover();
        await expect(
          page.locator(`circle[data-play-destination="${step.to}"]`),
        ).toBeVisible();
        await checker.click();
      }
      await expect(page.locator('.play-workspace')).toHaveAttribute(
        'aria-busy',
        'false',
      );
      const humanAudio = await page.evaluate(() => window.__nativeAudio.length);
      expect(humanAudio).toBeGreaterThanOrEqual(move.steps.length);
      const played = page.waitForResponse(
        (r) => r.url().endsWith('/actions') && r.request().method() === 'POST',
      );
      await page.locator('[data-dice-action="confirm"]').click();
      const next = await played;
      expect(next.status()).toBe(200);
      state = await next.json();
      expect(state.history.length).toBeGreaterThanOrEqual((turn + 1) * 2);
      expect(state.engine.version).toContain('1.08.003');
      if (state.phase === 'double_response') state = await takeDouble();
      await expect(page.locator('.play-workspace')).toHaveAttribute(
        'aria-busy',
        'false',
      );
      await expect
        .poll(() => page.evaluate(() => window.__nativeAudio.length))
        .toBeGreaterThan(humanAudio);
      expect(await page.evaluate(() => window.__nativeAudio)).toContain(0.28);
    }
    // Resolve the mandatory pre-roll decision before entering history review.
    // The ensuing unplayed human turn becomes
    // the checkpoint against which read-only review/reload is checked.
    if (state.phase === 'roll') {
      const rolling = page.waitForResponse(
        (r) => r.url().endsWith('/actions') && r.request().method() === 'POST',
      );
      await rollDice(page).click();
      const rolled = await rolling;
      expect(rolled.status(), await rolled.text()).toBe(200);
      state = await rolled.json();
    }
    if (state.phase === 'double_response') state = await takeDouble();
    expect(state.phase).toBe('move');
    const before = state;
    await page.reload();
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-direction',
      config.direction,
    );
    const restored = (await (
      await page.request.get(`/api/play/sessions/${state.id}`)
    ).json()) as PlaySession;
    expect(restored.version).toBe(before.version);
    expect(restored.position).toEqual(before.position);
    expect(await page.evaluate(() => window.__nativeAudio)).toEqual([]);
    // Review native SGF snapshots without submitting or altering a live turn.
    const reviewWrites: string[] = [];
    page.on('request', (request) => {
      if (request.url().includes('/api/play/') && request.method() !== 'GET')
        reviewWrites.push(request.url());
    });
    const history = await page.request.get(
      `/api/play/sessions/${state.id}/history?version=${state.version}`,
    );
    expect(history.status()).toBe(200);
    expect((await history.json()).items).toHaveLength(state.history.length);
    await page.locator('.play-history .move-history-row').first().click();
    await expect(
      page.locator('.play-history-review [role="status"]'),
    ).toHaveCount(0);
    await expect(
      page.locator('.play-history-review [role="alert"]'),
    ).toHaveCount(0);
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowDown');
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-review',
      '1',
    );
    await expect(page.locator('.board-shell')).toHaveAttribute(
      'data-preview-mode',
      'after',
    );
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.press('ArrowUp');
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-review',
      '0',
    );
    await expect(page.locator('.board-shell')).toHaveAttribute(
      'data-preview-mode',
      'before',
    );
    await page
      .getByRole('button', { name: 'Return to game', exact: true })
      .click();
    expect(
      await (await page.request.get(`/api/play/sessions/${state.id}`)).json(),
    ).toEqual(restored);
    expect(reviewWrites).toEqual([]);
    expect(await page.evaluate(() => window.__nativeAudio)).toEqual([]);
    const svg = await page.locator('.play-board .board').boundingBox();
    const home = await page
      .locator(`[data-play-point="${config.human === 0 ? 1 : 24}"]`)
      .boundingBox();
    expect(home!.y).toBeGreaterThan(svg!.y + svg!.height / 2);
    if (config.direction === 'left')
      expect(home!.x).toBeLessThan(svg!.x + svg!.width / 2);
    else expect(home!.x).toBeGreaterThan(svg!.x + svg!.width / 2);
    const sgf = await page.request.get(`/api/play/sessions/${state.id}/sgf`);
    expect(sgf.ok()).toBeTruthy();
    expect(await sgf.text()).toContain('GM[6]');
    await page.screenshot({
      path: `/tmp/bgtutor-native-${config.direction}.png`,
      fullPage: true,
    });
  });
}
