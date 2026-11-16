import { test, expect } from './test';
import { historyFixture } from './play-history-review-helpers';
import { botDice, playMenuItem } from './play-board-actions';

test('the action panel confirms a turn by button and by Enter', async ({
  page,
}) => {
  const { writes, point } = await historyFixture(page);
  const toolbar = page.getByTestId('play-board-toolbar');
  const confirm = toolbar.getByRole('button', {
    name: 'Confirm turn',
    exact: true,
  });
  const undo = toolbar.getByRole('button', { name: 'Undo step', exact: true });
  await expect(
    toolbar.getByRole('heading', { name: 'Move your checkers' }),
  ).toBeVisible();
  await expect(confirm).toBeDisabled();
  await expect(confirm).toHaveAttribute('aria-keyshortcuts', 'Enter');
  await expect(toolbar.locator('.play-turn-help')).toContainText(
    'Hover over a checker',
  );
  await page.keyboard.press('Enter');
  expect(writes).toEqual([]);
  await point(13).click();
  await expect(undo).toBeEnabled();
  await page.keyboard.press('z');
  await expect(undo).toBeDisabled();
  await expect(page.locator('.play-selected-steps')).toHaveCount(0);
  await point(13).click();
  await point(6).click();
  await expect(confirm).toBeEnabled();
  await expect(toolbar.locator('.play-turn-help')).toHaveText(
    'Click the dice pair to confirm your turn.',
  );
  await page.locator('.play-match-format').click();
  await page.keyboard.press('Enter');
  await expect
    .poll(() => writes)
    .toEqual([{ version: 7, action: 'move', move: 0 }]);
});

test('Roll, Double and Hint stay visible after the bot has rolled', async ({
  page,
}) => {
  const { saved, writes } = await historyFixture(page);
  Object.assign(saved, {
    phase: 'roll',
    play_mode: 'coach',
    can_double: true,
    legal_moves: [],
    position: { ...saved.position, dice: [] },
  });
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const toolbar = page.getByTestId('play-board-toolbar');
  const roll = toolbar.getByRole('button', { name: 'Roll dice', exact: true });
  await expect(botDice(page).first()).toBeVisible();
  await expect(
    toolbar.getByRole('heading', { name: 'Your turn' }),
  ).toBeVisible();
  await expect(roll).toBeEnabled();
  await expect(roll).toHaveAttribute('aria-keyshortcuts', 'Enter');
  await expect(
    toolbar.getByRole('button', { name: 'Double', exact: true }),
  ).toHaveAttribute('aria-keyshortcuts', 'D');
  await expect(
    toolbar.getByRole('button', { name: 'Hint', exact: true }),
  ).toHaveAttribute('aria-keyshortcuts', 'H');
  await page.keyboard.press('d');
  await expect.poll(() => writes).toEqual([{ version: 7, action: 'double' }]);
  await expect(roll).toBeEnabled();
  // Enter on the focused workspace tab belongs to the tab itself.
  await page.locator('.play-match-format').click();
  await page.keyboard.press('Enter');
  await expect
    .poll(() => writes)
    .toEqual([
      { version: 7, action: 'double' },
      { version: 7, action: 'roll' },
    ]);
});

test('a forced pass offers a Pass button', async ({ page }) => {
  const { saved, writes } = await historyFixture(page);
  Object.assign(saved, { legal_moves: [] });
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const toolbar = page.getByTestId('play-board-toolbar');
  await expect(
    toolbar.getByRole('heading', { name: 'No legal move — pass' }),
  ).toBeVisible();
  await expect(
    toolbar.getByRole('button', { name: 'Undo step', exact: true }),
  ).toHaveCount(0);
  await toolbar.getByRole('button', { name: 'Pass', exact: true }).click();
  await expect.poll(() => writes).toEqual([{ version: 7, action: 'pass' }]);
});

test('Escape returns from the move review to the game', async ({ page }) => {
  const { rows, writes } = await historyFixture(page);
  await rows.first().click();
  const back = page.getByRole('button', {
    name: 'Return to game',
    exact: true,
  });
  await expect(back).toHaveAttribute('aria-keyshortcuts', 'Escape');
  await page.locator('.play-match-format').click();
  await page.keyboard.press('Escape');
  await expect(back).toHaveCount(0);
  await expect(page.getByTestId('play-board-toolbar')).toBeVisible();
  expect(writes).toEqual([]);
});

test('the setup form marks the start action and opens the play settings', async ({
  page,
}) => {
  await historyFixture(page);
  await (await playMenuItem(page, 'New match')).click();
  const setup = page.locator('.play-setup');
  await expect(
    setup.getByRole('button', { name: 'Start match', exact: true }),
  ).toHaveClass(/\bprimary\b/);
  await setup.getByRole('radio', { name: 'Black', exact: true }).check();
  await expect(
    setup.getByRole('radio', { name: 'White', exact: true }),
  ).not.toBeChecked();
  await setup
    .getByRole('button', { name: 'Change engine in Settings', exact: true })
    .click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await expect(
    settings.getByRole('group', { name: 'Playing', exact: true }),
  ).toBeVisible();
  await page.keyboard.press('Escape');
  await setup
    .getByRole('button', { name: 'Back to match', exact: true })
    .click();
  await expect(page.getByTestId('play-board-toolbar')).toBeVisible();
});

test('a blocked Double stays visible and names the reason', async ({
  page,
}) => {
  const { saved, writes } = await historyFixture(page);
  Object.assign(saved, {
    phase: 'roll',
    can_double: false,
    legal_moves: [],
    position: { ...saved.position, dice: [] },
  });
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const double = page
    .getByTestId('play-board-toolbar')
    .getByRole('button', { name: 'Double', exact: true });
  await expect(double).toBeDisabled();
  await expect(double).toHaveAccessibleDescription(/\S/);
  expect(writes).toEqual([]);
});
