import { backendTest as test, expect } from '../helpers/test';
import { profileBackend } from '../helpers/profile-backend';
import { defaultConfiguration } from '../../src/settings/configuration/configuration-model';

test('imports legacy preferences once and preserves unrelated workspace state', async ({
  page,
}) => {
  const configuration = structuredClone(defaultConfiguration);
  configuration.play = {
    ...configuration.play,
    engine: 'bgsage',
    plies: 2,
    top_two: true,
  };
  const backend = await profileBackend(page, {
    configuration,
    initialized: false,
  });
  await page.addInitScript(() => {
    if (sessionStorage.getItem('legacy-installed')) return;
    sessionStorage.setItem('legacy-installed', 'yes');
    localStorage.setItem('neondojo.locale', 'de');
    localStorage.setItem('neondojo.theme', 'walnut');
    localStorage.setItem(
      'neondojo.ui-appearance',
      JSON.stringify({ version: 1, theme: 'paper', mode: 'dark' }),
    );
    localStorage.setItem('neondojo.play-sounds', 'off');
    localStorage.setItem(
      'neondojo.library.visibleColumns',
      JSON.stringify({ version: 2, columns: ['id'] }),
    );
    localStorage.setItem('neondojo.jobs.unrelated', 'kept');
  });
  const settings = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/settings' &&
      response.request().method() === 'GET',
  );
  await page.goto('/');
  expect(await (await settings).json()).toMatchObject({
    play_top_two: true,
    gnubg_threads: null,
    gnubg_play_threads: null,
    open_sage_threads: null,
    open_sage_play_threads: null,
    parallel_analysis_jobs: null,
  });
  await expect(
    page.getByRole('button', { name: /Profil: Standard/ }),
  ).toBeVisible();
  expect(backend.state.current_configuration.general.locale).toBe('de');
  expect(backend.state.current_configuration.appearance.board_theme).toBe(
    'walnut',
  );
  expect(backend.state.current_configuration.general.play_sounds).toBe(false);
  expect(backend.state.current_configuration.play.top_two).toBe(true);
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
    await page.evaluate(() => localStorage.getItem('neondojo.jobs.unrelated')),
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
      'neondojo.configuration-pending.v1',
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
