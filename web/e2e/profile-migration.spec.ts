import { test, expect } from '@playwright/test';
import { profileBackend } from './profile-backend';

test('imports legacy preferences once and preserves unrelated workspace state', async ({
  page,
}) => {
  const backend = await profileBackend(page, { initialized: false });
  await page.addInitScript(() => {
    if (sessionStorage.getItem('legacy-installed')) return;
    sessionStorage.setItem('legacy-installed', 'yes');
    localStorage.setItem('bgtutor.locale', 'de');
    localStorage.setItem('bgtutor.theme', 'walnut');
    localStorage.setItem(
      'bgtutor.ui-appearance',
      JSON.stringify({ version: 1, theme: 'paper', mode: 'dark' }),
    );
    localStorage.setItem('bgtutor.play-sounds', 'off');
    localStorage.setItem(
      'bgtutor.library.visibleColumns',
      JSON.stringify({ version: 1, columns: ['id'] }),
    );
    localStorage.setItem('bgtutor.jobs.unrelated', 'kept');
  });
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profil: Standard/ }),
  ).toBeVisible();
  expect(backend.state.current_configuration.general.locale).toBe('de');
  expect(backend.state.current_configuration.appearance.board_theme).toBe(
    'walnut',
  );
  expect(backend.state.current_configuration.general.play_sounds).toBe(false);
  expect(backend.state.current_configuration.library.visible_columns).toEqual([
    'id',
  ]);
  await page.reload();
  await expect(
    page.getByRole('button', { name: /Profil: Standard/ }),
  ).toBeVisible();
  expect(
    backend.calls.filter((c) => c.path === '/configuration/initialize'),
  ).toHaveLength(1);
  expect(
    await page.evaluate(() => localStorage.getItem('bgtutor.jobs.unrelated')),
  ).toBe('kept');
});

test('recovers pending current settings without overwriting the saved profile', async ({
  page,
}) => {
  const backend = await profileBackend(page);
  const pending = structuredClone(backend.state.current_configuration);
  pending.general.player_name = 'Recovered local name';
  await page.addInitScript((config) => {
    localStorage.setItem(
      'bgtutor.configuration-pending.v1',
      JSON.stringify({ base_revision: 1, configuration: config }),
    );
  }, pending);
  await page.goto('/');
  await expect(
    page.getByRole('button', { name: /Profile: Standard/ }),
  ).toContainText('modified');
  expect(backend.state.current_configuration.general.player_name).toBe(
    'Recovered local name',
  );
  expect(backend.profiles[0].configuration.general.player_name).toBe('');
});
