import { test, expect, type Page } from './test';
import {
  historyFixture,
  historySessionId as id,
} from './play-history-review-helpers';
import { nativeSteps } from '../src/play-sound-events';
const decisionId = '56e78413-1d0c-45ed-bcc9-43e48968cc09';
async function fixture(page: Page, mode = 'coach', human = 0, revealed = true) {
  const base = await historyFixture(page, human);
  const { saved } = base;
  saved.legal_moves = ['13/10 6/5', '13/9'].map((notation) => ({
    notation,
    steps: nativeSteps(notation, human),
  }));
  Object.assign(saved, {
    play_mode: mode,
    pending_decision: {
      id: decisionId,
      action: 'move',
      move_index: 0,
      original_action: 'move',
      original_move_index: 0,
      warning: mode === 'tutor',
      revealed,
      assisted: false,
      retry_count: 0,
      loss: 0.03,
      first_loss: 0.03,
      severity: 'doubtful',
      first_severity: 'doubtful',
      attempts: [],
      committed: false,
      alternatives: saved.legal_moves.map((move, index) => ({
        action: 'move',
        move_index: index,
        notation: move.notation,
        loss: index * 0.03,
        equity: 0.777,
        probabilities: [0.8, 0.1, 0, 0.2, 0.05, 0],
      })),
    },
  });
  await page.reload();
  await page.locator('#workspace-tab-play').click();
  return base;
}
for (const config of [
  { mode: 'coach', human: 0, width: 1440 },
  { mode: 'coach', human: 1, width: 1440 },
  { mode: 'tutor', human: 0, width: 390 },
  { mode: 'tutor', human: 1, width: 1440 },
])
  test(`${config.mode} candidate preview preserves the pending move ${config.human}/${config.width}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width: config.width, height: 1000 });
    const { saved, writes } = await fixture(page, config.mode, config.human);
    await page.route(`**/api/play/sessions/${id}/actions`, (r) => {
      writes.push(r.request().postDataJSON());
      return r.fulfill({
        json: {
          ...saved,
          version: 8,
          phase: 'roll',
          can_double: true,
          pending_decision: null,
          position: { ...saved.position, dice: [] },
        },
      });
    });
    const dialog = page.getByRole('alertdialog', {
      name: config.mode === 'coach' ? 'Coach' : 'Tutor',
      exact: true,
    });
    await expect(dialog).toBeVisible();
    const preview = dialog.locator('.play-decision-board');
    await expect(preview.locator('.move-arrows')).toBeVisible();
    await expect(preview.locator('[data-play-point]')).toHaveCount(0);
    if (config.mode === 'coach')
      await expect(dialog).not.toContainText('0.777');
    const second = dialog.getByRole('button', {
      name: 'Preview move 2: 13/9',
      exact: true,
    });
    await second.click();
    await expect(second).toHaveAttribute('aria-pressed', 'true');
    await second.press('ArrowUp');
    await expect(
      dialog.getByRole('button', {
        name: 'Preview move 1: 13/10 6/5',
        exact: true,
      }),
    ).toHaveAttribute('aria-pressed', 'true');
    await second.click();
    await dialog.getByRole('button', { name: 'Final', exact: true }).click();
    await expect(preview.locator('.board-shell')).toHaveAttribute(
      'data-preview-mode',
      'after',
    );
    await expect(
      preview.locator(`[data-point="${config.human ? 16 : 9}"]`).first(),
    ).toHaveAttribute('data-count', config.human ? '-1' : '1');
    expect(writes).toEqual([]);
    await page.keyboard.press('Escape');
    await expect(dialog).toBeVisible();
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(config.width);
    await page.screenshot({
      path: `/tmp/bgtutor-feedback-preview-${config.mode}-${config.human}.png`,
      fullPage: true,
    });
    await dialog
      .getByRole('button', {
        name: config.mode === 'tutor' ? 'Play anyway' : 'Continue',
        exact: true,
      })
      .click();
    await expect(dialog).toHaveCount(0);
    expect(writes).toEqual([
      { version: 7, action: 'move', move: 0, decision_id: decisionId },
    ]);
  });
test('Tutor preview is hidden until Show solution and retry returns to the board', async ({
  page,
}) => {
  const { saved, writes } = await fixture(page, 'tutor', 0, false);
  const feedback: unknown[] = [];
  await page.route(`**/api/play/sessions/${id}/feedback`, (r) => {
    const data = r.request().postDataJSON();
    feedback.push(data);
    const decision = saved.pending_decision!;
    Object.assign(saved, {
      version: saved.version + 1,
      pending_decision: {
        ...decision,
        revealed: true,
        ...(data.operation === 'retry'
          ? { action: null, move_index: null }
          : {}),
      },
    });
    return r.fulfill({ json: saved });
  });
  const dialog = page.getByRole('alertdialog', { name: 'Tutor', exact: true });
  await expect(dialog).toBeVisible();
  await expect(dialog.locator('.play-decision-board')).toHaveCount(0);
  await expect(dialog).not.toContainText('13/9');
  await dialog
    .getByRole('button', { name: 'Show solution', exact: true })
    .click();
  await expect(
    dialog.locator('.play-decision-board .move-arrows'),
  ).toBeVisible();
  await dialog
    .getByRole('button', { name: 'Preview move 2: 13/9', exact: true })
    .click();
  await dialog.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('[data-play-point="13"]')).toHaveAttribute(
    'data-enabled',
    'true',
  );
  expect(writes).toEqual([]);
  expect(feedback).toEqual([
    { version: 7, operation: 'reveal', decision_id: decisionId },
    { version: 8, operation: 'retry', decision_id: decisionId },
  ]);
});
test('Competition pending confirmation never exposes a candidate preview', async ({
  page,
}) => {
  await fixture(page, 'competition');
  const dialog = page.getByRole('alertdialog', {
    name: 'Competition',
    exact: true,
  });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText('Unconfirmed decision');
  await expect(dialog.locator('.play-decision-board')).toHaveCount(0);
  await expect(dialog.locator('.candidate-preview-button')).toHaveCount(0);
  await expect(dialog).not.toContainText('13/9');
});

test('switching revealed Coach feedback to Competition removes the preview', async ({
  page,
}) => {
  const { saved, writes } = await fixture(page);
  await page.route(`**/api/play/sessions/${id}/mode`, (r) => {
    expect(r.request().postDataJSON()).toEqual({
      version: 7,
      play_mode: 'competition',
    });
    return r.fulfill({
      json: { ...saved, version: 8, play_mode: 'competition' },
    });
  });
  const dialog = page.getByRole('alertdialog', { name: 'Coach', exact: true });
  await expect(dialog.locator('.play-decision-board')).toBeVisible();
  await dialog.locator('.play-dialog-preferences summary').click();
  await dialog
    .getByLabel('Change play mode', { exact: true })
    .selectOption('competition');
  const neutral = page.getByRole('alertdialog', {
    name: 'Competition',
    exact: true,
  });
  await expect(neutral).toContainText('Unconfirmed decision');
  await expect(neutral.locator('.play-decision-board')).toHaveCount(0);
  await expect(neutral.locator('.candidate-preview-button')).toHaveCount(0);
  await expect(neutral).not.toContainText('13/9');
  expect(writes).toEqual([]);
});
