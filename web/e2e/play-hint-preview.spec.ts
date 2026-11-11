import { test, expect, type Page } from '@playwright/test';
import {
  historyFixture,
  historySessionId as id,
} from './play-history-review-helpers';
import { nativeSteps } from '../src/play-sound-events';
const pendingId = 'e1146df1-d424-483b-9301-1327b492e5af';
const hint = (alternatives: unknown[]) => ({
  id: pendingId,
  action: null,
  original_action: null,
  move_index: null,
  original_move_index: null,
  warning: false,
  revealed: true,
  assisted: true,
  retry_count: 0,
  loss: null,
  first_loss: null,
  severity: null,
  first_severity: null,
  alternatives,
  attempts: [],
  committed: false,
});
async function fixture(page: Page, human = 0, phase = 'move') {
  const base = await historyFixture(page, human);
  const { saved } = base;
  Object.assign(saved, {
    phase,
    play_mode: 'coach',
    can_double: phase === 'roll',
  });
  saved.legal_moves = ['13/10 6/5', '13/9'].map((notation) => ({
    notation,
    steps: nativeSteps(notation, human),
  }));
  if (phase !== 'move') saved.position.dice = [];
  const feedbackWrites: unknown[] = [];
  await page.route(`**/api/play/sessions/${id}/feedback`, (r) => {
    feedbackWrites.push(r.request().postDataJSON());
    Object.assign(saved, {
      version: saved.version + 1,
      pending_decision: hint(
        phase === 'move'
          ? saved.legal_moves.map((m, i) => ({
              action: 'move',
              move_index: i,
              notation: m.notation,
              loss: i * 0.03,
              equity: 0.777,
              probabilities: [0.8, 0.1, 0, 0.2, 0.05, 0],
            }))
          : [
              {
                action: 'take',
                move_index: null,
                notation: 'Take',
                loss: 0,
                equity: 0.3,
                probabilities: [0.6, 0.1, 0, 0.4, 0.1, 0],
              },
            ],
      ),
    });
    return r.fulfill({ json: saved });
  });
  await page.reload();
  await page.locator('#workspace-tab-play').click();
  return { ...base, feedbackWrites };
}
for (const human of [0, 1])
  test(`Hint previews candidates without changing partial turn ${human}`, async ({
    page,
  }) => {
    const { point, writes, feedbackWrites } = await fixture(page, human);
    await point(13).click();
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    await page.getByRole('button', { name: 'Hint', exact: true }).click();
    const modal = page.getByRole('dialog', { name: 'Hint', exact: true });
    await expect(modal).toBeVisible();
    expect(await modal.evaluate((el) => el.matches(':modal'))).toBe(true);
    const preview = modal.locator('.play-decision-board');
    await expect(preview.locator('.board-shell')).toHaveAttribute(
      'data-preview-mode',
      'arrows',
    );
    await expect(preview.locator('.move-arrows')).toBeVisible();
    await expect(preview.locator('[data-play-point]')).toHaveCount(0);
    await expect(modal).not.toContainText('0.777');
    const second = modal.getByRole('button', {
      name: 'Preview move 2: 13/9',
      exact: true,
    });
    await second.click();
    await expect(second).toHaveAttribute('aria-pressed', 'true');
    await second.press('ArrowUp');
    await expect(
      modal.getByRole('button', {
        name: 'Preview move 1: 13/10 6/5',
        exact: true,
      }),
    ).toHaveAttribute('aria-pressed', 'true');
    await second.click();
    await modal.getByRole('button', { name: 'Final', exact: true }).click();
    await expect(preview.locator('.board-shell')).toHaveAttribute(
      'data-preview-mode',
      'after',
    );
    const destination = human ? 16 : 9;
    await expect(
      preview.locator(`[data-point="${destination}"]`).first(),
    ).toHaveAttribute('data-count', human ? '-1' : '1');
    await modal.getByRole('button', { name: 'Initial', exact: true }).click();
    await expect(preview.locator('.board-shell')).toHaveAttribute(
      'data-preview-mode',
      'before',
    );
    await page.screenshot({
      path: `/tmp/bgtutor-hint-${human}.png`,
      fullPage: true,
    });
    await modal.getByRole('button', { name: 'Close', exact: true }).click();
    await expect(modal).toHaveCount(0);
    await expect(
      page.getByRole('button', { name: 'Hint', exact: true }),
    ).toBeFocused();
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    await point(6).click();
    await expect(page.locator('[data-dice-action="confirm"]')).toHaveCount(2);
    expect(writes).toEqual([]);
    expect(feedbackWrites).toEqual([{ version: 7, operation: 'hint' }]);
  });
for (const phase of ['roll', 'double_response'])
  test(`closing cube Hint restores required ${phase} dialog`, async ({
    page,
  }) => {
    const { writes } = await fixture(page, 0, phase);
    await page.getByRole('button', { name: 'Hint', exact: true }).click();
    const modal = page.getByRole('dialog', { name: 'Hint', exact: true });
    await expect(modal).toBeVisible();
    await expect(modal.locator('.play-decision-board')).toHaveCount(0);
    await expect(modal.getByTestId('play-feedback')).toContainText('Take');
    await expect(page.locator('dialog[open]')).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(modal).toHaveCount(0);
    const underlying = page.getByRole('alertdialog', {
      name: phase === 'roll' ? 'Next roll' : 'Cube decision',
      exact: true,
    });
    await expect(underlying).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(underlying).toBeVisible();
    expect(writes).toEqual([]);
  });
test('Hint loading and errors stay in the view and can be closed', async ({
  page,
}) => {
  const { writes } = await fixture(page);
  let release = () => {};
  const gate = new Promise<void>((resolve) => (release = resolve));
  await page.route(`**/api/play/sessions/${id}/feedback`, async (r) => {
    await gate;
    await r.fulfill({
      status: 503,
      json: { error: 'Hint engine unavailable' },
    });
  });
  await page.getByRole('button', { name: 'Hint', exact: true }).click();
  const modal = page.getByRole('dialog', { name: 'Hint', exact: true });
  await expect(modal).toBeVisible();
  await expect(modal.getByRole('status')).toContainText('Loading hint');
  release();
  await expect(modal.getByRole('alert')).toContainText(
    'Hint engine unavailable',
  );
  await expect(modal.locator('.candidate-preview-button')).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(modal).toHaveCount(0);
  expect(writes).toEqual([]);
});
test('mobile Hint fits viewport and an invalid move never changes the preview board', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { saved } = await fixture(page);
  await page.route(`**/api/play/sessions/${id}/feedback`, (r) =>
    r.fulfill({
      json: {
        ...saved,
        version: 8,
        pending_decision: hint([
          { action: 'move', move_index: 0, notation: '24/off', loss: 0 },
        ]),
      },
    }),
  );
  await page.getByRole('button', { name: 'Hint', exact: true }).click();
  const modal = page.getByRole('dialog', { name: 'Hint', exact: true });
  await expect(modal).toBeVisible();
  await expect(modal).toContainText('cannot be previewed safely');
  await expect(
    modal.getByRole('button', { name: 'Final', exact: true }),
  ).toBeDisabled();
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(390);
  await page.screenshot({
    path: '/tmp/bgtutor-hint-mobile.png',
    fullPage: true,
  });
  await modal.getByRole('button', { name: 'Close', exact: true }).click();
});

for (const exit of ['close', 'workspace'] as const)
  test(`a delayed hint does not reopen after ${exit}`, async ({ page }) => {
    const { saved, writes, point } = await fixture(page);
    await point(13).click();
    let release = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    await page.route(`**/api/play/sessions/${id}/feedback`, async (r) => {
      await gate;
      await r.fulfill({
        json: {
          ...saved,
          version: 8,
          pending_decision: hint([
            { action: 'move', move_index: 0, notation: '13/10 6/5', loss: 0 },
          ]),
        },
      });
    });
    await page.getByRole('button', { name: 'Hint', exact: true }).click();
    const modal = page.getByRole('dialog', { name: 'Hint', exact: true });
    await expect(modal).toBeVisible();
    if (exit === 'close')
      await modal.getByRole('button', { name: 'Close', exact: true }).click();
    else {
      await modal.getByRole('button', { name: 'Close', exact: true }).click();
      await page.getByRole('tab', { name: 'Analysis', exact: true }).click();
    }
    release();
    if (exit === 'workspace')
      await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await expect(page.locator('.play-workspace')).toHaveAttribute(
      'aria-busy',
      'false',
    );
    await expect(modal).toHaveCount(0);
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    expect(writes).toEqual([]);
  });
test('Training hint shows full analysis and a usable preview on mobile', async ({
  page,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { saved, writes } = await fixture(page);
  Object.assign(saved, { play_mode: 'training' });
  await page.reload();
  await page.locator('#workspace-tab-play').click();
  await page.getByRole('button', { name: 'Hint', exact: true }).click();
  const modal = page.getByRole('dialog', { name: 'Hint', exact: true });
  await expect(modal).toBeVisible();
  await expect(modal.getByTestId('play-feedback')).toContainText('0.777');
  await expect(modal.getByTestId('play-feedback')).toContainText('80%');
  await expect(
    modal.locator('.play-decision-board .move-arrows'),
  ).toBeVisible();
  await modal
    .getByRole('button', { name: 'Preview move 2: 13/9', exact: true })
    .click();
  await modal.getByRole('button', { name: 'Final', exact: true }).click();
  await expect(
    modal.locator('.play-decision-board .board-shell'),
  ).toHaveAttribute('data-preview-mode', 'after');
  const bounds = (await modal.boundingBox())!;
  expect(bounds.x).toBeGreaterThanOrEqual(15);
  expect(bounds.x + bounds.width).toBeLessThanOrEqual(375);
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(390);
  await page.screenshot({
    path: '/tmp/bgtutor-hint-mobile-training.png',
    fullPage: true,
  });
  await modal.getByRole('button', { name: 'Close', exact: true }).click();
  expect(writes).toEqual([]);
});

test('retrying a failed hint preserves the partial turn', async ({ page }) => {
  const { saved, point, writes } = await fixture(page);
  await point(13).click();
  let requests = 0;
  await page.route(`**/api/play/sessions/${id}/feedback`, (r) =>
    ++requests === 1
      ? r.fulfill({ status: 503, json: { error: 'Transient hint failure' } })
      : r.fulfill({
          json: {
            ...saved,
            version: 8,
            pending_decision: hint([
              { action: 'move', move_index: 0, notation: '13/10 6/5', loss: 0 },
            ]),
          },
        }),
  );
  await page.getByRole('button', { name: 'Hint', exact: true }).click();
  const modal = page.getByRole('dialog', { name: 'Hint', exact: true });
  await expect(modal.getByRole('alert')).toContainText(
    'Transient hint failure',
  );
  await modal.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(modal.locator('.move-arrows')).toBeVisible();
  await modal.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
  await expect(
    page.getByRole('button', { name: 'Hint', exact: true }),
  ).toBeFocused();
  expect(requests).toBe(2);
  expect(writes).toEqual([]);
});
