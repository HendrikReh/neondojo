import { test, expect, type Page } from './test';
import {
  historyFixture,
  historySessionId as id,
} from './play-history-review-helpers';
const decisionId = '7216ea44-6ad0-4e8c-a543-f809a63de1d1';
const pending = {
  id: decisionId,
  action: 'roll',
  original_action: 'roll',
  move_index: null,
  original_move_index: null,
  warning: false,
  revealed: true,
  assisted: false,
  retry_count: 0,
  loss: 0,
  first_loss: 0,
  severity: 'none',
  first_severity: 'none',
  alternatives: [],
  attempts: [],
  committed: false,
};
async function reload(page: Page) {
  await page.reload();
  await page.locator('#workspace-tab-play').click();
}
async function mandatory(page: Page, title: string) {
  const dialog = page.getByRole('alertdialog', { name: title, exact: true });
  await expect(dialog).toBeVisible();
  expect(await dialog.evaluate((el) => el.matches(':modal'))).toBe(true);
  const board = await page.locator('.play-board .board').boundingBox();
  const box = await dialog.boundingBox();
  if (box!.width < board!.width)
    expect(
      Math.abs(box!.x + box!.width / 2 - board!.x - board!.width / 2),
    ).toBeLessThan(3);
  expect(box!.x).toBeGreaterThanOrEqual(15);
  expect(box!.x + box!.width).toBeLessThanOrEqual(
    page.viewportSize()!.width - 15,
  );
  // A tall dialog is clamped to the viewport, otherwise it follows the board center.
  if (box!.height < board!.height)
    expect(
      Math.abs(box!.y + box!.height / 2 - board!.y - board!.height / 2),
    ).toBeLessThan(3);
  await page.keyboard.press('Escape');
  await page.mouse.click(5, 5);
  await expect(dialog).toBeVisible();
  return dialog;
}
for (const width of [1440, 390])
  test(`Rolling in the toolbar still requires Coach acknowledgment at ${width}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1100 });
    const { saved } = await historyFixture(page);
    Object.assign(saved, {
      phase: 'roll',
      can_double: true,
      play_mode: 'coach',
    });
    saved.position.dice = [];
    const writes: unknown[] = [];
    await page.route(`**/api/play/sessions/${id}/actions`, (r) => {
      const data = r.request().postDataJSON();
      writes.push(data);
      Object.assign(
        saved,
        writes.length === 1
          ? { version: 8, pending_decision: pending }
          : {
              version: 9,
              phase: 'move',
              pending_decision: null,
              can_double: false,
            },
      );
      if (writes.length === 2) saved.position.dice = [3, 1];
      return r.fulfill({ json: saved });
    });
    await reload(page);
    const next = page.getByTestId('play-board-toolbar');
    await expect(page.getByRole('alertdialog')).toHaveCount(0);
    await page.screenshot({
      path: `/tmp/bgtutor-next-roll-${width}.png`,
      fullPage: true,
    });
    await next.getByRole('button', { name: 'Roll dice', exact: true }).click();
    const coach = page
      .getByTestId('play-board-toolbar')
      .getByTestId('play-feedback');
    await expect(coach).toBeVisible();
    await page.screenshot({
      path: `/tmp/bgtutor-coach-modal-${width}.png`,
      fullPage: true,
    });
    await expect(page.getByRole('alertdialog')).toHaveCount(0);
    await coach.getByRole('button', { name: 'Continue', exact: true }).click();
    await expect(page.getByRole('alertdialog')).toHaveCount(0);
    await expect(page.locator('[data-play-point="13"]')).toHaveAttribute(
      'data-enabled',
      'true',
    );
    expect(writes).toEqual([
      { version: 7, action: 'roll' },
      { version: 8, action: 'roll', decision_id: decisionId },
    ]);
  });
test('closing a Hint after a Tutor retry leaves the board free of feedback', async ({
  page,
}) => {
  await page.setViewportSize({ width: 1800, height: 1169 });
  const { saved } = await historyFixture(page);
  const firstAttempt = {
    ...pending,
    original_action: 'move',
    original_move_index: 0,
    original_notation: '13/10 6/5',
  };
  Object.assign(saved, {
    play_mode: 'tutor',
    pending_decision: {
      ...firstAttempt,
      action: 'move',
      move_index: 0,
      warning: true,
      revealed: false,
    },
  });
  await page.route(`**/api/play/sessions/${id}/feedback`, (r) => {
    expect(r.request().postDataJSON()).toEqual({
      version: saved.version,
      operation: saved.version === 7 ? 'retry' : 'hint',
      decision_id: decisionId,
    });
    Object.assign(saved, {
      version: saved.version + 1,
      pending_decision: {
        ...firstAttempt,
        action: null,
        revealed: saved.version === 8,
        assisted: true,
        retry_count: 1,
      },
    });
    return r.fulfill({ json: saved });
  });
  await reload(page);
  const dialog = await mandatory(page, 'Tutor');
  await dialog.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(page.getByRole('alertdialog')).toHaveCount(0);
  const hint = page.getByRole('button', { name: 'Hint', exact: true });
  await hint.click();
  const hintDialog = page.getByRole('dialog', { name: 'Hint', exact: true });
  await expect(hintDialog.getByTestId('play-feedback')).toContainText(
    '13/10 6/5',
  );
  await hintDialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(hintDialog).toHaveCount(0);
  await expect(page.getByTestId('play-feedback')).toHaveCount(0);
  await expect(hint).toBeFocused();
  await expect(page.locator('[data-play-point="13"]')).toHaveAttribute(
    'data-enabled',
    'true',
  );
});
test('forced pass and game boundaries use the same modal flow', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  Object.assign(saved, { legal_moves: [] });
  const writes: unknown[] = [];
  await page.route(`**/api/play/sessions/${id}/actions`, (r) => {
    writes.push(r.request().postDataJSON());
    Object.assign(
      saved,
      writes.length === 1
        ? { version: 8, phase: 'game_over', winner: 1 }
        : { version: 9, phase: 'opening_roll', winner: null },
    );
    return r.fulfill({ json: saved });
  });
  await reload(page);
  let dialog = await mandatory(page, 'No legal move — pass');
  await dialog
    .getByRole('button', { name: 'No legal move — pass', exact: true })
    .click();
  dialog = await mandatory(page, 'Game finished');
  await expect(dialog).toContainText('Winner: Bot');
  await dialog.getByRole('button', { name: 'Next game', exact: true }).click();
  dialog = await mandatory(page, 'Opening roll');
  await expect(
    dialog.getByRole('button', { name: 'Roll one die each' }),
  ).toBeEnabled();
  expect(writes).toEqual([
    { version: 7, action: 'pass' },
    { version: 8, action: 'next_game' },
  ]);
});
test('match result can be acknowledged to access learning review', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  Object.assign(saved, { phase: 'match_over', winner: 0 });
  await reload(page);
  const dialog = await mandatory(page, 'Match finished');
  await dialog
    .getByRole('button', { name: 'Back to board', exact: true })
    .click();
  await expect(page.getByRole('alertdialog')).toHaveCount(0);
  await expect(
    page.getByRole('button', { name: 'New match', exact: true }),
  ).toBeEnabled();
});

test('a finished match opens the new match setup directly', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  Object.assign(saved, { phase: 'match_over', winner: 0 });
  await reload(page);
  const dialog = page.getByRole('alertdialog', { name: 'Match finished' });
  const newMatch = dialog.getByRole('button', {
    name: 'New match',
    exact: true,
  });
  await expect(newMatch).toHaveClass(/\bprimary\b/);
  await newMatch.click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'New match' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Start match' })).toBeEnabled();
  await page.getByRole('button', { name: 'Resume match', exact: true }).click();
  await expect(page.locator('.play-board .board')).toBeVisible();
  await expect(dialog).toHaveCount(0);
});

test('pending Tutor dialog keeps live mode and threshold saving accessible', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  Object.assign(saved, {
    play_mode: 'tutor',
    tutor_threshold: 0.04,
    pending_decision: {
      ...pending,
      action: 'move',
      original_action: 'move',
      move_index: 0,
    },
  });
  const writes: unknown[] = [];
  let release = () => {};
  const gate = new Promise<void>((resolve) => (release = resolve));
  await page.route(`**/api/play/sessions/${id}/mode`, async (r) => {
    const data = r.request().postDataJSON();
    writes.push(data);
    if (writes.length === 1) await gate;
    Object.assign(saved, {
      version: saved.version + 1,
      play_mode: data.play_mode,
      tutor_threshold: data.tutor_threshold ?? 0.08,
    });
    await r.fulfill({ json: saved });
  });
  await reload(page);
  const dialog = await mandatory(page, 'Tutor');
  await dialog.locator('.play-dialog-preferences summary').click();
  const threshold = dialog.getByLabel('Tutor warning threshold', {
    exact: true,
  });
  await threshold.focus();
  const tooltip = dialog.getByRole('tooltip');
  await expect(tooltip).toContainText(
    'Equity loss that triggers Tutor warnings',
  );
  await page.keyboard.press('Escape');
  await expect(tooltip).toHaveCount(0);
  await expect(threshold).toBeFocused();
  await threshold.fill('0.08');
  await threshold.press('Tab');
  await expect(dialog.locator('.play-threshold-status')).toHaveText('Saving…');
  release();
  await expect(dialog.locator('.play-threshold-status')).toHaveText('Saved');
  await dialog
    .getByLabel('Change play mode', { exact: true })
    .selectOption('coach');
  await expect(
    page.getByTestId('play-board-toolbar').getByTestId('play-feedback'),
  ).toBeVisible();
  expect(writes).toEqual([
    { version: 7, play_mode: 'tutor', tutor_threshold: 0.08 },
    { version: 8, play_mode: 'coach' },
  ]);
  await expect(page.getByRole('alertdialog')).toHaveCount(0);
});

test('Roll failure and reload stay beside the board', async ({ page }) => {
  const { saved } = await historyFixture(page);
  Object.assign(saved, { phase: 'roll', can_double: true });
  await page.route(`**/api/play/sessions/${id}/actions`, (r) =>
    r.fulfill({ status: 503, json: { error: 'Native roll failure' } }),
  );
  await reload(page);
  const toolbar = page.getByTestId('play-board-toolbar');
  await toolbar.getByRole('button', { name: 'Roll dice', exact: true }).click();
  await expect(toolbar.getByRole('alert')).toContainText('Native roll failure');
  await toolbar
    .getByRole('button', { name: 'Reload saved state', exact: true })
    .click();
  await expect(toolbar.getByRole('alert')).toHaveCount(0);
  await expect(
    toolbar.getByRole('button', { name: 'Roll dice', exact: true }),
  ).toBeEnabled();
});
