import { backendTest as test, expect } from './test';
import { profileBackend } from './profile-backend';
import { configurationJournalKey } from '../src/configuration-store';

test('Apply changes the visible board despite an older pending configuration', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  const pending = structuredClone(backend.state.current_configuration);
  pending.library.filters.q = 'Unsaved local filter';
  const remote = structuredClone(backend.state.current_configuration);
  remote.general.player_name = 'Saved in another window';
  backend.updateRemote(remote);
  await page.addInitScript(
    ({ key, pending }) => {
      localStorage.setItem(
        key,
        JSON.stringify({ base_revision: 1, configuration: pending }),
      );
    },
    { key: configurationJournalKey, pending },
  );

  await page.goto('/');
  await page
    .getByLabel('Match file', { exact: true })
    .setInputFiles('../backend/tests/fixtures/tutorial.mat');
  const board = page.locator('.analysis-workspace .board-panel');
  await expect(board.locator('.theme-forest .board')).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings.getByRole('button', { name: 'Board', exact: true }).click();
  await settings.getByRole('button', { name: 'Walnut', exact: true }).click();
  await expect(board.locator('.theme-forest .board')).toBeVisible();
  await settings.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(
    settings.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeHidden();
  await expect(settings).toBeVisible();
  await expect(
    settings.getByRole('button', { name: 'Board', exact: true }),
  ).toHaveAttribute('aria-current', 'page');
  await expect(
    settings.getByRole('button', { name: 'Walnut', exact: true }),
  ).toHaveAttribute('aria-pressed', 'true');
  await expect(board.locator('.theme-walnut .board')).toBeVisible();
  await settings.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(settings).toBeHidden();
  await expect(
    page.getByRole('dialog', { name: 'Unapplied settings' }),
  ).toBeHidden();
  expect(backend.state.current_configuration).toEqual({
    ...remote,
    appearance: { ...remote.appearance, board_theme: 'walnut' },
  });
  const journal = await page.evaluate(
    (key) => JSON.parse(localStorage.getItem(key)!),
    configurationJournalKey,
  );
  expect(journal.base_revision).toBe(1);
  expect(journal.configuration.library.filters.q).toBe('Unsaved local filter');
});

test('applying a board theme uses the latest configuration without prompting', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  const openSettings = page.getByRole('button', {
    name: 'Settings',
    exact: true,
  });
  await openSettings.click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings.getByRole('button', { name: 'Board', exact: true }).click();
  const walnut = settings.getByRole('button', { name: 'Walnut', exact: true });
  await walnut.click();

  const remote = structuredClone(backend.state.current_configuration);
  remote.appearance.board_theme = 'slate';
  remote.appearance.mode = 'dark';
  remote.general.locale = 'de';
  remote.general.player_name = 'Changed in another window';
  backend.updateRemote(remote);

  await settings.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(page.locator('html')).toHaveAttribute('lang', 'de');
  const localizedSettings = page.getByRole('dialog', {
    name: 'Einstellungen',
    exact: true,
  });
  await expect(
    localizedSettings.getByRole('button', { name: 'Anwenden', exact: true }),
  ).toBeHidden();
  await expect(
    localizedSettings.getByRole('button', { name: 'Walnut', exact: true }),
  ).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('dialog')).toHaveCount(1);
  await expect(page.locator('html')).toHaveAttribute('data-ui-mode', 'dark');
  expect(backend.state.current_configuration).toEqual({
    ...remote,
    appearance: { ...remote.appearance, board_theme: 'walnut' },
  });
});

test('an unresolved conflict offers explicit recovery and preserves the appearance draft', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings
    .getByRole('button', { name: 'Appearance', exact: true })
    .click();
  await settings
    .getByRole('combobox', { name: 'Interface theme', exact: true })
    .selectOption('club');
  await settings.getByRole('button', { name: 'Close', exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Unapplied settings' });
  backend.failNext(
    409,
    'revision_conflict',
    'Configuration changed',
    '/configuration',
  );
  await guard.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(guard.getByRole('alert')).toContainText(
    'Settings changed elsewhere',
  );
  await expect(
    settings.getByRole('combobox', { name: 'Interface theme', exact: true }),
  ).toHaveValue('club');
  const remote = structuredClone(backend.state.current_configuration);
  remote.appearance.ui_theme = 'paper';
  remote.appearance.mode = 'dark';
  backend.updateRemote(remote);
  const loadLatest = guard.getByRole('button', {
    name: 'Load latest and discard draft',
    exact: true,
  });
  backend.failNext(503, 'test_failure', 'Request failed', '/configuration');
  await loadLatest.click();
  await expect(guard.getByRole('alert')).toContainText(
    'Settings could not be loaded.',
  );
  await expect(
    settings.getByRole('combobox', { name: 'Interface theme', exact: true }),
  ).toHaveValue('club');
  await loadLatest.click();
  await expect(guard).toBeHidden();
  await expect(
    settings.getByRole('combobox', { name: 'Interface theme', exact: true }),
  ).toHaveValue('paper');
  await expect(
    settings.getByRole('combobox', { name: 'Display mode', exact: true }),
  ).toHaveValue('dark');
});

test('settings drafts follow a profile activated inside the settings dialog', async ({
  page,
}) => {
  await profileBackend(page);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings.getByRole('button', { name: 'Profiles', exact: true }).click();
  await settings
    .getByRole('list', { name: 'Profiles', exact: true })
    .getByRole('button', { name: /Deep analysis/ })
    .click();
  await settings.getByRole('button', { name: 'Activate', exact: true }).click();
  await expect(
    page.getByRole('button', { name: /Profile: Deep analysis/ }),
  ).toBeVisible();
  await settings.getByRole('button', { name: 'Analysis', exact: true }).click();
  await expect(
    settings.getByLabel('Default analysis profile', { exact: true }),
  ).toHaveValue('4');
  await expect(
    settings.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeHidden();
  await settings.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(settings).not.toBeVisible();
  await expect(
    page.getByRole('button', { name: /Profile: Deep analysis/ }),
  ).not.toContainText('modified');
});

test('opening book conflicts preserve the draft and offer explicit recovery', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page
    .getByRole('dialog', { name: 'Settings', exact: true })
    .getByRole('button', { name: 'Analysis', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Manage opening books', exact: true })
    .click();
  const book = page.getByRole('dialog', { name: 'Opening books', exact: true });
  await book
    .getByLabel('Opening-book source', { exact: true })
    .selectOption('off');
  backend.failNext(
    409,
    'revision_conflict',
    'Configuration changed',
    '/configuration',
  );
  await book.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(
    book.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeDisabled();
  await expect(
    book.getByLabel('Opening-book source', { exact: true }),
  ).toHaveValue('off');
  await expect(
    book.getByRole('button', { name: /Reload|Retry|Try again|latest/i }),
  ).toBeVisible();
});
