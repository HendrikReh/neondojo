import { test, expect } from './test';
import { botDice, doubleCube } from './play-board-actions';
import {
  historyFixture,
  historySessionId as id,
} from './play-history-review-helpers';

for (const width of [1440, 390])
  test(`resign sits below Hint and Undo and cancellation preserves the partial turn at ${width}`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    const { saved, writes, point } = await historyFixture(page);
    saved.play_mode = 'coach';
    await page.reload();
    await page.getByRole('tab', { name: 'Playing', exact: true }).click();
    await point(13).click();
    const toolbar = page.getByTestId('play-board-toolbar');
    const resign = toolbar.getByRole('button', { name: 'Resign', exact: true });
    await expect(resign).toBeVisible();
    const undoBox = (await toolbar
      .getByRole('button', { name: 'Undo step', exact: true })
      .boundingBox())!;
    const box = (await resign.boundingBox())!;
    expect(box.y).toBeGreaterThanOrEqual(undoBox.y + undoBox.height);
    await expect(resign.locator('.action-label')).toHaveText('Resign');
    await resign.click();
    const dialog = page.getByRole('dialog', {
      name: 'Resign this game',
      exact: true,
    });
    await expect(dialog).toBeVisible();
    await expect(
      dialog.getByRole('button', { name: 'Resign game', exact: true }),
    ).toBeVisible();
    await page.screenshot({
      path: `/tmp/bgtutor-resign-${width}.png`,
      fullPage: true,
    });
    expect(
      await page.evaluate(() => document.documentElement.scrollWidth),
    ).toBeLessThanOrEqual(width);
    await page.keyboard.press('Escape');
    await expect(dialog).toHaveCount(0);
    await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
    await expect(resign).toBeFocused();
    expect(writes).toEqual([]);
  });

test('a selected resignation submits exactly one versioned action and shows native game result', async ({
  page,
}) => {
  const { saved, writes } = await historyFixture(page);
  await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
    writes.push(route.request().postDataJSON());
    await route.fulfill({
      json: {
        ...saved,
        version: 8,
        phase: 'game_over',
        winner: 1,
        scores: [0, 2],
        legal_moves: [],
        pending_decision: null,
        game_results: [
          {
            game_no: 1,
            history_index: 2,
            result: {
              winner: 1,
              points: 2,
              cube: 1,
              multiplier: 2,
              reason: 'resignation',
              scores_after: [0, 2],
            },
          },
        ],
      },
    });
  });
  await page.getByRole('button', { name: 'Resign', exact: true }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Resign this game',
    exact: true,
  });
  await dialog.getByRole('radio', { name: 'Gammon', exact: true }).check();
  await expect(dialog.getByRole('status')).toContainText('2 × Cube 1 = 2');
  await dialog
    .getByRole('button', { name: 'Resign game', exact: true })
    .dblclick();
  await expect(dialog).toHaveCount(0);
  await expect(
    page.getByRole('alertdialog', { name: 'Game finished', exact: true }),
  ).toContainText('Bot wins 2 points');
  expect(writes).toEqual([{ version: 7, action: 'resign', resign_level: 2 }]);
});

test('a declined offer keeps the selected level, error and unconfirmed turn until cancelled', async ({
  page,
}) => {
  const { writes, point } = await historyFixture(page);
  await point(13).click();
  await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
    writes.push(route.request().postDataJSON());
    await route.fulfill({
      status: 422,
      json: {
        error:
          'The bot declined this resignation. Choose a higher level or continue playing; the saved game is unchanged',
      },
    });
  });
  await page.getByRole('button', { name: 'Resign', exact: true }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Resign this game',
    exact: true,
  });
  await dialog
    .getByRole('button', { name: 'Resign game', exact: true })
    .click();
  await expect(dialog.getByRole('alert')).toContainText('The bot declined');
  await expect(
    dialog.getByRole('button', { name: 'Reload saved state', exact: true }),
  ).toHaveCount(0);
  await expect(
    dialog.getByRole('radio', { name: 'Single game', exact: true }),
  ).toBeChecked();
  await dialog
    .getByRole('button', { name: 'Continue playing', exact: true })
    .click();
  await expect(page.locator('.play-selected-steps')).toHaveText('13/10');
  expect(writes).toEqual([{ version: 7, action: 'resign', resign_level: 1 }]);
});

test('cancelling resignation restores the board controls before the roll', async ({
  page,
}) => {
  const { saved, writes } = await historyFixture(page);
  Object.assign(saved, {
    phase: 'roll',
    can_double: true,
    legal_moves: [],
    position: { ...saved.position, dice: [] },
  });
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const roll = page.getByTestId('play-board-toolbar');
  await roll.getByRole('button', { name: 'Resign', exact: true }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Resign this game',
    exact: true,
  });
  await expect(dialog).toBeVisible();
  await dialog
    .getByRole('button', { name: 'Continue playing', exact: true })
    .click();
  await expect(botDice(page).first()).toBeEnabled();
  await expect(doubleCube(page)).toBeEnabled();
  await expect(
    roll.getByRole('button', { name: 'Resign', exact: true }),
  ).toBeFocused();
  expect(writes).toEqual([]);
});

test('German resignation shows the active cube and its exact award', async ({
  page,
}) => {
  const { saved, writes } = await historyFixture(page);
  saved.match_length = 5;
  const match = Buffer.from(saved.position.gnubg_match_id, 'base64');
  match[0] = (match[0] & 0xf0) | 1;
  saved.position.gnubg_match_id = match.toString('base64').replace(/=+$/, '');
  await expect(
    page.locator('.profile-selector > button').first(),
  ).toContainText('Standard');
  await page.evaluate(async () => {
    const state = await (await fetch('/api/configuration')).json();
    const response = await fetch('/api/configuration', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        expected_revision: state.revision,
        configuration: {
          ...state.current_configuration,
          general: { ...state.current_configuration.general, locale: 'de' },
        },
      }),
    });
    if (!response.ok) throw new Error(await response.text());
  });
  await page.reload();
  await page.locator('#workspace-tab-play').click();
  await page.getByRole('button', { name: 'Aufgeben', exact: true }).click();
  const dialog = page.getByRole('dialog', {
    name: 'Diese Partie aufgeben',
    exact: true,
  });
  await dialog.getByRole('radio', { name: 'Gammon', exact: true }).check();
  await expect(dialog.getByRole('status')).toContainText('2 × Cube 2 = 4');
  await expect(dialog.getByRole('status')).toContainText(
    'Bot erhält bei Annahme 4 Punkte.',
  );
  await expect(dialog.getByRole('status')).toContainText(
    'Matchstand bei Annahme',
  );
  await expect(dialog.getByRole('status')).toContainText('Human 0 : 4 Bot');
  await expect(dialog.getByRole('status')).toContainText(
    'Bei Annahme wird das Match fortgesetzt.',
  );
  await dialog.getByRole('radio', { name: 'Backgammon', exact: true }).check();
  await expect(
    dialog.getByRole('button', { name: 'Match aufgeben', exact: true }),
  ).toBeVisible();
  await dialog.getByRole('radio', { name: 'Gammon', exact: true }).check();
  await expect(
    dialog.getByRole('button', { name: 'Partie aufgeben', exact: true }),
  ).toBeVisible();
  await dialog
    .getByRole('button', { name: 'Weiterspielen', exact: true })
    .click();
  expect(writes).toEqual([]);
});

for (const human of [0, 1])
  test(`resignation previews score and match completion for player ${human}`, async ({
    page,
  }) => {
    await page.setViewportSize({
      width: human === 0 ? 1440 : 390,
      height: 900,
    });
    const { saved, writes } = await historyFixture(page, human);
    saved.scores = human === 0 ? [3, 2] : [3, 4];
    const match = Buffer.from(saved.position.gnubg_match_id, 'base64');
    match[0] = (match[0] & 0xf0) | 1;
    saved.position.gnubg_match_id = match.toString('base64').replace(/=+$/, '');
    await page.reload();
    await page.locator('#workspace-tab-play').click();
    await page.getByRole('button', { name: 'Resign', exact: true }).click();
    const dialog = page.getByRole('dialog', {
      name: 'Resign this game',
      exact: true,
    });
    const status = dialog.getByRole('status');
    await expect(status).toContainText('Match score if accepted');
    await expect(
      dialog.getByRole('button', { name: 'Resign game', exact: true }),
    ).toBeVisible();
    await expect(status).toContainText(
      human === 0 ? 'Human 3 : 4 Bot' : 'Bot 5 : 4 Human',
    );
    await expect(status).toContainText('Match to 7 points');
    await expect(status).toContainText('If accepted, the match continues.');
    await dialog.getByRole('radio', { name: 'Gammon', exact: true }).check();
    await expect(status).toContainText(
      human === 0 ? 'Human 3 : 6 Bot' : 'Bot 7 : 4 Human',
    );
    await expect(status).toContainText(
      human === 0
        ? 'If accepted, the match continues.'
        : 'If accepted, the match ends. Bot wins.',
    );
    await dialog
      .getByRole('radio', { name: 'Backgammon', exact: true })
      .check();
    await expect(status).toContainText(
      human === 0 ? 'Human 3 : 8 Bot' : 'Bot 9 : 4 Human',
    );
    await expect(status).toContainText(
      'If accepted, the match ends. Bot wins.',
    );
    await page.screenshot({
      path: `/tmp/bgtutor-resign-match-score-${human}.png`,
      fullPage: true,
    });
    await dialog
      .getByRole('radio', { name: 'Single game', exact: true })
      .check();
    await expect(status).toContainText('If accepted, the match continues.');
    await expect(status).not.toContainText('the match ends');
    await expect(
      dialog.getByRole('button', { name: 'Resign game', exact: true }),
    ).toBeVisible();
    await dialog
      .getByRole('button', { name: 'Continue playing', exact: true })
      .click();
    expect(writes).toEqual([]);
  });
