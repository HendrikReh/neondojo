import { test, expect } from './test';
import { historyFixture } from './play-history-review-helpers';
import { botDice, playMenuItem, resignGame } from './play-board-actions';

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
    .getByRole('button', { name: 'Engine settings', exact: true })
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

test('the setup dialog overlays the match and sends this match’s name, engine and strength', async ({
  page,
}) => {
  const { saved } = await historyFixture(page);
  await page.route('**/api/engines', (route) =>
    route.fulfill({
      json: [
        { id: 'gnubg', name: 'GNUbg', available: true, plies: [1, 2, 3] },
        { id: 'bgsage', name: 'NeonGammon', available: true, plies: [1, 2] },
      ],
    }),
  );
  const created: unknown[] = [];
  await page.route('**/api/play/sessions', (route) => {
    if (route.request().method() !== 'POST') return route.fallback();
    created.push(route.request().postDataJSON());
    return route.fulfill({
      status: 201,
      json: { ...saved, human_name: 'Guest' },
    });
  });
  await (await playMenuItem(page, 'New match')).click();
  const dialog = page.getByRole('dialog', { name: 'New match', exact: true });
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
  await expect(dialog).toHaveClass(/\bplay-setup-dialog\b/);
  // The running match stays rendered behind the modal dialog.
  await expect(page.locator('.play-live-board .board')).toBeVisible();
  await expect(page.getByTestId('play-board-toolbar')).toBeAttached();
  const name = dialog.getByRole('textbox', { name: 'Your name', exact: true });
  await name.fill('Guest');
  const engine = dialog.getByRole('combobox', {
    name: 'Bot engine',
    exact: true,
  });
  const strength = dialog.getByRole('combobox', {
    name: 'Playing strength',
    exact: true,
  });
  await expect(strength.locator('option')).toHaveCount(3);
  await engine.selectOption('bgsage');
  await expect(strength.locator('option')).toHaveText(['1-ply', '2-ply']);
  await strength.selectOption('2');
  await dialog
    .getByRole('button', { name: 'Start match', exact: true })
    .click();
  await expect(dialog).toHaveCount(0);
  expect(created).toEqual([
    expect.objectContaining({
      human_name: 'Guest',
      play_engine: 'bgsage',
      play_plies: 2,
    }),
  ]);
});

test('without a match the setup dialog can be closed and reopened', async ({
  page,
}) => {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'New match', exact: true });
  await expect(dialog).toBeVisible();
  await expect(page.locator('.play-idle-board .board')).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  const reopen = page
    .locator('.play-header')
    .getByRole('button', { name: 'New match', exact: true });
  await expect(reopen).toBeFocused();
  await reopen.click();
  await expect(dialog).toBeVisible();
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(dialog).toHaveCount(0);
});

test('a deleted stored session is forgotten and opens the setup dialog', async ({
  page,
}) => {
  const missing = '00000000-0000-4000-8000-000000000404';
  await page.addInitScript((id) => {
    localStorage.setItem('neondojo.locale', 'en');
    localStorage.setItem('neondojo.play-session', id);
  }, missing);
  let reads = 0;
  await page.route(`**/api/play/sessions/${missing}`, (route) => {
    reads++;
    return route.fulfill({ status: 404, json: { error: 'Not found' } });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(
    page.getByRole('dialog', { name: 'New match', exact: true }),
  ).toBeVisible();
  await expect(page.locator('.play-error')).toHaveCount(0);
  expect(reads).toBe(1);
  expect(
    await page.evaluate(() => localStorage.getItem('neondojo.play-session')),
  ).toBeNull();
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

test('a money session names resignations and its score without match wording', async ({
  page,
}) => {
  const { saved, rows } = await historyFixture(page);
  saved.match_length = 0;
  saved.history.push({ player: 1, dice: [], notation: 'resign' });
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(rows.last().locator('strong')).toHaveText('Resign');
  const mode = page.getByLabel('Change play mode', { exact: true });
  await mode.hover();
  await expect(page.getByRole('tooltip')).toContainText(
    'Analysis and pip counts stay hidden',
  );
  await resignGame(page).click();
  const dialog = page.getByRole('dialog', {
    name: 'Resign this game',
    exact: true,
  });
  await expect(dialog).toContainText('Score if accepted');
  await expect(dialog).toContainText('If accepted, the session continues.');
  await expect(dialog).not.toContainText('match continues');
});
