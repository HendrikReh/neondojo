import { test, expect, type Page } from './test';
const id = 'b2175e29-daf1-40c2-8100-52df618287a4';
const decisionId = '7216ea44-6ad0-4e8c-a543-f809a63de1d1';
function state(mode = 'competition') {
  return {
    id,
    version: 0,
    human_player: 0,
    human_name: 'Learner',
    match_length: 7,
    bearoff_direction: 'right',
    play_mode: mode,
    engine: { id: 'gnubg', version: 'GNU Backgammon native', plies: 0 },
    phase: 'double_response',
    position: {
      id,
      match_uuid: id,
      ordinal: 0,
      game_no: 1,
      ply: 0,
      player: 0,
      dice: [],
      board: [
        0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0,
        0, 2, 0,
      ],
      position_id: '4HPwATDgc/ABMA',
      gnubg_match_id: 'cBGgAAAAAAAE',
      notation: '',
    },
    legal_moves: [],
    can_double: false,
    history: [{ player: 1, dice: [], notation: 'double' }],
    scores: [0, 0],
    game_no: 1,
    winner: null,
    pending_decision: null as unknown,
  };
}
async function resume(page: Page, saved: ReturnType<typeof state>) {
  await page.addInitScript((id) => {
    localStorage.setItem('bgtutor.play-session', id);
    localStorage.setItem('bgtutor.play-sounds', 'off');
  }, id);
  await page.route(`**/api/play/sessions/${id}`, (r) =>
    r.fulfill({ json: saved }),
  );
  await page.goto('/');
  await page.locator('#workspace-tab-play').click();
}
for (const width of [1440, 390])
  test(`cube response is mandatory and anchored to the board at ${width}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 1100 });
    const saved = state();
    const writes: unknown[] = [];
    await page.route(`**/api/play/sessions/${id}/actions`, (r) => {
      writes.push(r.request().postDataJSON());
      return r.fulfill({ json: { ...saved, version: 1, phase: 'roll' } });
    });
    await resume(page, saved);
    const dialog = page.getByRole('alertdialog', {
      name: 'Cube decision',
      exact: true,
    });
    await expect(dialog).toBeVisible();
    expect(
      await dialog.evaluate((el) =>
        (el as HTMLDialogElement).matches(':modal'),
      ),
    ).toBe(true);
    const board = (await page.locator('.play-board .board').boundingBox())!,
      box = (await dialog.boundingBox())!;
    const viewport = page.viewportSize()!;
    const gutter = 16;
    // The side toolbar shifts the mobile board left. Keep the modal as
    // close to the board center as the viewport gutters allow.
    const expectedX = Math.max(
      gutter,
      Math.min(
        board.x + (board.width - box.width) / 2,
        viewport.width - box.width - gutter,
      ),
    );
    const expectedY = Math.max(
      gutter,
      Math.min(
        board.y + (board.height - box.height) / 2,
        viewport.height - box.height - gutter,
      ),
    );
    expect(Math.abs(box.x - expectedX)).toBeLessThan(3);
    expect(Math.abs(box.y - expectedY)).toBeLessThan(3);
    expect(box.x).toBeGreaterThanOrEqual(gutter - 1);
    expect(box.x + box.width).toBeLessThanOrEqual(viewport.width - gutter + 1);
    await page.keyboard.press('Escape');
    await page.mouse.click(5, 5);
    await expect(dialog).toBeVisible();
    expect(writes).toEqual([]);
    for (let i = 0; i < 6; i++) {
      await page.keyboard.press('Tab');
      expect(
        await dialog.evaluate((el) => el.contains(document.activeElement)),
      ).toBe(true);
    }
    await page.screenshot({
      path: `/tmp/bgtutor-cube-dialog-${width}.png`,
      fullPage: true,
    });
    await dialog
      .getByRole('button', { name: 'Take double', exact: true })
      .click();
    await expect(dialog).toHaveCount(0);
    expect(writes).toEqual([{ version: 0, action: 'take' }]);
  });
test('cube dialog keeps Tutor warning and explicit acknowledgment accessible', async ({
  page,
}) => {
  const saved = state('tutor');
  const writes: unknown[] = [];
  await page.route(`**/api/play/sessions/${id}/actions`, (r) => {
    const data = r.request().postDataJSON();
    writes.push(data);
    Object.assign(
      saved,
      writes.length === 1
        ? {
            version: 1,
            pending_decision: {
              id: decisionId,
              action: 'take',
              original_action: 'take',
              move_index: null,
              original_move_index: null,
              warning: true,
              revealed: false,
              assisted: false,
              retry_count: 0,
              loss: null,
              first_loss: null,
              severity: null,
              first_severity: null,
              alternatives: [],
              attempts: [],
              committed: false,
            },
          }
        : { version: 2, phase: 'roll', pending_decision: null },
    );
    return r.fulfill({ json: saved });
  });
  await resume(page, saved);
  const dialog = page.getByRole('alertdialog', {
    name: 'Cube decision',
    exact: true,
  });
  await dialog
    .getByRole('button', { name: 'Take double', exact: true })
    .click();
  await expect(dialog.getByTestId('play-feedback')).toContainText('reconsider');
  await dialog
    .getByRole('button', { name: 'Play anyway', exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  expect(writes).toEqual([
    { version: 0, action: 'take' },
    { version: 1, action: 'take', decision_id: decisionId },
  ]);
});
test('native failure is visible inside the mandatory dialog', async ({
  page,
}) => {
  await page.route(`**/api/play/sessions/${id}/actions`, (r) =>
    r.fulfill({ status: 503, json: { error: 'Native cube failure' } }),
  );
  await resume(page, state());
  const dialog = page.getByRole('alertdialog', {
    name: 'Cube decision',
    exact: true,
  });
  await dialog
    .getByRole('button', { name: 'Pass double', exact: true })
    .click();
  await expect(dialog.getByRole('alert')).toContainText('Native cube failure');
  await expect(
    dialog.getByRole('button', { name: 'Reload saved state', exact: true }),
  ).toBeEnabled();
  await page.keyboard.press('Escape');
  await expect(dialog).toBeVisible();
});
