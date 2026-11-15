import { watchOpening } from './play-opening-helpers';
import { test, expect } from '@playwright/test';
import type { PlaySession } from '../src/play-api';
import {
  matchingCheckerTurns,
  nativeLegalTurns,
  nextCheckerSteps,
} from '../src/play-state';
test.skip(
  !process.env.BGTUTOR_FEEDBACK_NATIVE_TEST,
  'Requires isolated native feedback backend',
);
test.beforeEach(async ({ page }) => {
  // Initialize and configure the actual persisted profile, without API mocks.
  await page.goto('/');
  await expect(page.getByRole('button', { name: /^Profile:/ })).toBeVisible();
  const current = await page.request.get('/api/configuration');
  expect(current.status()).toBe(200);
  const state = await current.json();
  const configuration = state.current_configuration;
  configuration.analysis.plies = 0;
  configuration.play.plies = 0;
  configuration.general.play_sounds = false;
  const updated = await page.request.put('/api/configuration', {
    data: { expected_revision: state.revision, configuration },
  });
  expect(updated.status(), await updated.text()).toBe(200);
});
for (const mode of ['competition', 'training', 'coach', 'tutor'] as const) {
  test(`native feedback lifecycle ${mode}`, async ({ page }) => {
    test.setTimeout(120000);
    let response = await page.request.post('/api/play/sessions', {
      data: {
        human_player: 0,
        match_length: 1,
        play_mode: mode,
        tutor_threshold: 0.001,
        training_auto_analysis: true,
      },
    });
    expect(response.status()).toBe(201);
    let state = (await response.json()) as PlaySession;
    const uri = `/api/play/sessions/${state.id}`;
    async function action(action: string, move?: number, decision_id?: string) {
      const response = await page.request.post(`${uri}/actions`, {
        data: {
          version: state.version,
          action,
          ...(move == null ? {} : { move }),
          ...(decision_id ? { decision_id } : {}),
        },
      });
      expect(response.status(), await response.text()).toBe(200);
      state = await response.json();
      return state;
    }
    while (['opening_roll', 'opening_result'].includes(state.phase))
      await action(
        state.phase === 'opening_roll' ? 'opening_roll' : 'start_opening_move',
      );
    if (state.phase === 'roll') {
      await action('roll');
      if (state.pending_decision)
        await action('roll', undefined, state.pending_decision.id);
    }
    expect(state.phase).toBe('move');
    expect(state.legal_moves.length).toBeGreaterThan(1);
    const checkpoint = JSON.stringify(state.position);
    const history = state.history.length;
    const moveIndex = state.legal_moves.length - 1;
    if (mode === 'coach') {
      const turns = nativeLegalTurns(
        state.position.board,
        state.human_player,
        state.position.dice,
        state.legal_moves,
      );
      await page.addInitScript((id) => {
        localStorage.setItem('bgtutor.play-session', id);
        localStorage.setItem('bgtutor.play-sounds', 'off');
      }, state.id);
      await page.goto('/');
      await page.getByRole('tab', { name: 'Playing', exact: true }).click();
      // The native legal catalog is ranked at 0 ply, the captured analysis depth.
      const turn = turns
        .toSorted((a, b) => b.moveIndex - a.moveIndex)
        .find((turn) => {
          const steps = turn.steps.map((step, index) => ({
            ...step,
            die: turn.dice[index],
          }));
          return steps.every(
            (step, index) =>
              nextCheckerSteps(
                matchingCheckerTurns(turns, steps.slice(0, index)),
                index,
                state.position.dice[index],
              ).get(step.from)?.die === step.die,
          );
        })!;
      for (const step of turn.steps) {
        await page
          .locator(`[data-play-point="${step.from}"] .play-point`)
          .click();
      }
      const confirm = page.locator('[data-dice-action="confirm"]');
      const confirmMove = async () => {
        const result = page.waitForResponse(
          (r) =>
            r.url().endsWith(`${uri}/actions`) &&
            r.request().method() === 'POST',
        );
        await confirm.click();
        const response = await result;
        expect(response.status(), await response.text()).toBe(200);
        return (await response.json()) as PlaySession;
      };
      const staged = await confirmMove();
      const pending = staged.pending_decision!;
      expect(pending).toMatchObject({
        original_move_index: turn.moveIndex,
        committed: false,
        revealed: true,
        loss: expect.any(Number),
      });
      expect(pending.loss).toBeGreaterThanOrEqual(0.02);
      expect(staged.position).toEqual(state.position);
      expect(staged.history).toEqual(state.history);
      const panel = page.getByTestId('play-feedback');
      await expect(panel).toBeVisible();
      await page.reload();
      await page.getByRole('tab', { name: 'Playing', exact: true }).click();
      await expect(panel).toBeVisible();
      const after = await confirmMove();
      expect(after.pending_decision).toBeNull();
      expect(after.history.length).toBeGreaterThan(history);
      expect(after.decisions?.at(-1)).toMatchObject({
        id: pending.id,
        original_move_index: turn.moveIndex,
        committed: true,
        revealed: true,
        loss: pending.loss,
      });
      await expect(page.getByRole('button', { name: 'Play move' })).toHaveCount(
        0,
      );
      await expect(
        page.locator(`[data-play-history-index="${history}"]`),
      ).toBeVisible();
      return;
    }
    await action('move', moveIndex);
    await page.addInitScript((id) => {
      localStorage.setItem('bgtutor.play-session', id);
      localStorage.setItem('bgtutor.play-sounds', 'off');
    }, state.id);
    await page.goto('/');
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    if (mode === 'competition') {
      await expect(page.getByTestId('play-feedback')).toHaveCount(0);
      expect(state.history.length).toBeGreaterThan(history);
      await expect(
        page.locator('.play-board .player-bar').first(),
      ).not.toContainText('pips');
      return;
    }
    expect(JSON.stringify(state.position)).toBe(checkpoint);
    expect(state.history.length).toBe(history);
    const pending = state.pending_decision!;
    expect(pending.action).toBe('move');
    const panel = page.getByTestId('play-feedback');
    await expect(panel).toBeVisible();
    if (mode === 'training')
      await expect(
        panel.getByRole('columnheader', { name: 'Equity', exact: true }),
      ).toBeVisible();
    if (mode === 'tutor') {
      await expect(panel).toContainText('reconsider');
      expect(pending.alternatives).toHaveLength(0);
      await page.locator('.play-dialog-preferences > summary').click();
      const threshold = page.getByRole('spinbutton', {
        name: 'Tutor warning threshold',
        exact: true,
      });
      for (const value of ['1', '0.001']) {
        const changed = page.waitForResponse((r) =>
          r.url().endsWith(`${uri}/mode`),
        );
        await threshold.fill(value);
        await threshold.press('Tab');
        const result = await changed;
        expect(result.status(), await result.text()).toBe(200);
        state = await result.json();
        expect(JSON.stringify(state.position)).toBe(checkpoint);
        expect(state.history.length).toBe(history);
        expect(state.pending_decision?.id).toBe(pending.id);
        await expect(threshold).toHaveValue(value);
        if (value === '1') {
          await expect(panel).not.toContainText('reconsider');
          await expect(
            panel.getByRole('button', { name: 'Continue', exact: true }),
          ).toBeVisible();
        } else await expect(panel).toContainText('reconsider');
      }
      await panel
        .getByRole('button', { name: 'Show solution', exact: true })
        .click();
      await expect(panel.getByRole('table')).toBeVisible();
    }
    await page.reload();
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await expect(panel).toBeVisible();
    const committed = page.waitForResponse(
      (r) =>
        r.url().endsWith(`${uri}/actions`) && r.request().method() === 'POST',
    );
    await panel
      .getByRole('button', {
        name: mode === 'tutor' ? 'Play anyway' : 'Continue',
        exact: true,
      })
      .click();
    const result = await committed;
    expect(result.status()).toBe(200);
    const after = (await result.json()) as PlaySession;
    expect(after.pending_decision).toBeNull();
    expect(after.history.length).toBeGreaterThan(history);
    expect(after.decisions?.at(-1)?.original_move_index).toBe(
      pending.original_move_index,
    );
    expect(after.decisions?.at(-1)?.first_loss).toBeCloseTo(
      pending.first_loss ?? after.decisions!.at(-1)!.first_loss!,
      6,
    );
    await expect(panel).toHaveCount(0);
    await page.screenshot({
      path: `/tmp/bgtutor-native-${mode}.png`,
      fullPage: true,
    });
  });
}

test('native Competition startup and live modes preserve the running position', async ({
  page,
}) => {
  test.setTimeout(120000);
  const response = await page.request.post('/api/play/sessions', {
    data: { human_player: 0, match_length: 1, play_mode: 'competition' },
  });
  expect(response.status()).toBe(201);
  const state = (await response.json()) as PlaySession;
  expect(state.library_match_id).toBeTruthy();
  await page.addInitScript((session) => {
    localStorage.setItem(
      'bgtutor.workspace',
      JSON.stringify({
        matchId: session.library_match_id,
        gameNo: 1,
      }),
    );
    localStorage.setItem('bgtutor.play-sounds', 'off');
  }, state);
  const restrictedReads: string[] = [];
  page.on('response', (response) => {
    if (response.status() === 422) restrictedReads.push(response.url());
  });
  const opened = watchOpening(page);
  await page.goto('/');
  await expect(
    page.getByRole('tab', { name: 'Playing', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  const afterOpening = await opened();
  async function showModeControl() {
    if (afterOpening.phase === 'double_response')
      await page.locator('.play-dialog-preferences > summary').click();
  }
  await showModeControl();
  const selector = page.getByRole('combobox', {
    name: 'Change play mode',
    exact: true,
  });
  await expect(selector).toHaveValue('competition');
  for (const mode of ['coach', 'tutor', 'training', 'competition'] as const) {
    const changed = page.waitForResponse((r) =>
      r.url().endsWith(`/play/sessions/${state.id}/mode`),
    );
    await selector.selectOption(mode);
    const result = await changed;
    expect(result.status(), await result.text()).toBe(200);
    const next = (await result.json()) as PlaySession;
    expect(next.position).toEqual(afterOpening.position);
    expect(next.history).toEqual(afterOpening.history);
    expect(next.phase).toEqual(afterOpening.phase);
    await expect(selector).toHaveValue(mode);
    await expect(
      page.getByRole('spinbutton', {
        name: 'Tutor warning threshold',
        exact: true,
      }),
    ).toHaveCount(mode === 'tutor' ? 1 : 0);
  }
  await page.reload();
  await expect(
    page.getByRole('tab', { name: 'Playing', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  await showModeControl();
  await expect(selector).toHaveValue('competition');
  await expect(page.getByRole('alert')).toHaveCount(0);
  expect(restrictedReads).toEqual([]);
});
