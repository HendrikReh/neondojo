import { waitForJobCompletion } from '../helpers/match-fixtures';
import { DESKTOP_VIEWPORTS } from '../helpers/navigation';
import type { components } from '../../src/generated-api';
import {
  fulfillJson,
  mockSettings,
  mockEngineCatalog,
  settingsFixture,
} from '../helpers/api-fixtures';
import { applyAndCloseSettings } from '../helpers/settings-actions';
import {
  calculateDiceEquity,
  openFullMatchAnalysis,
  openMatchAnalysisOptions,
  openPositionCalculation,
  submitFullMatchAnalysis,
} from '../helpers/analysis-controls';
import { backendTest, test, expect, type Page } from '../helpers/test';
import { profileBackend } from '../helpers/profile-backend';
import { installedBoardThemes as themes } from '../helpers/board-themes';

const initialSettings = {
  ...settingsFixture(),
  version: 3,
  player_name: 'Settings Player',
  analysis_plies: 1,
  play_plies: 2,
  gnubg_path: '/opt/bin/gnubg',
};

async function settingsBackend(
  page: Page,
  analysisPlies = initialSettings.analysis_plies,
) {
  let saved = { ...initialSettings, analysis_plies: analysisPlies };
  let conflict = false;
  const writes: components['schemas']['AppSettings'][] = [];
  await mockSettings(page, ({ request, respond, fail }) => {
    if (request.method() === 'GET') return respond(saved);
    const body = request.postDataJSON();
    writes.push(body);
    if (conflict) return fail(409, { error: 'Version changed' });
    saved = { ...saved, ...body, version: saved.version + 1 };
    return respond(saved);
  });
  await page.route('**/api/configuration', async (route) => {
    if (route.request().method() !== 'PUT') return route.fallback();
    const c = route.request().postDataJSON().configuration;
    const proposed = {
      ...saved,
      player_name: c.general.player_name,
      analysis_engine: c.analysis.engine,
      analysis_plies: c.analysis.plies,
      play_engine: c.play.engine,
      play_plies: c.play.plies,
    };
    writes.push(proposed);
    if (conflict)
      return route.fulfill({
        status: 409,
        json: {
          error: 'Version changed',
          code: 'revision_conflict',
          issues: [],
        },
      });
    saved = { ...proposed, version: saved.version + 1 };
    return route.fallback();
  });
  await mockEngineCatalog(page);
  await page.route('**/api/engines/gnubg/check', (route) =>
    fulfillJson<'ErrorBody'>(route, { error: 'Executable not found' }, 422),
  );
  return {
    writes,
    conflict: (value: boolean) => {
      conflict = value;
    },
  };
}

async function openSettings(page: Page) {
  const trigger = page.getByRole('button', { name: 'Settings', exact: true });
  await trigger.click();
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  await expect(dialog.getByLabel('Player name', { exact: true })).toHaveValue(
    initialSettings.player_name,
  );
  return { dialog, trigger };
}

test('keyboard switches settings categories and respects the profile draft guard', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  await page.goto('/');
  const { dialog } = await openSettings(page);
  const sidebar = dialog.getByRole('navigation', {
    name: 'Settings categories',
  });
  const general = sidebar.getByRole('button', { name: 'General', exact: true });
  const appearance = sidebar.getByRole('button', {
    name: 'Appearance',
    exact: true,
  });
  const profiles = sidebar.getByRole('button', {
    name: 'Profiles',
    exact: true,
  });
  const name = dialog.getByLabel('Player name', { exact: true });

  await general.press('ArrowDown');
  await expect(appearance).toBeFocused();
  await expect(appearance).toHaveAttribute('aria-current', 'page');
  await page.keyboard.press('ArrowUp');
  await expect(general).toBeFocused();
  await expect(name).toBeVisible();
  await page.keyboard.press('End');
  await expect(profiles).toBeFocused();
  await expect(profiles).toHaveAttribute('aria-current', 'page');
  await page.keyboard.press('Home');
  await expect(general).toBeFocused();
  await expect(general).toHaveAttribute('aria-current', 'page');

  await name.fill('Keyboard draft');
  await name.press('Home');
  await name.press('End');
  await expect(name).toBeFocused();
  await general.press('End');
  const guard = page.getByRole('dialog', { name: 'Unapplied settings' });
  await guard
    .getByRole('button', { name: 'Keep editing', exact: true })
    .click();
  await expect(general).toBeFocused();
  await expect(name).toHaveValue('Keyboard draft');
  await page.keyboard.press('End');
  await guard
    .getByRole('button', { name: 'Discard draft', exact: true })
    .click();
  await expect(profiles).toBeFocused();
  await expect(profiles).toHaveAttribute('aria-current', 'page');
  await page.keyboard.press('Home');
  await expect(name).toHaveValue(initialSettings.player_name);
  await profiles.click();
  const footer = dialog.locator('.settings-footer');
  await expect(
    footer.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeHidden();
  await footer.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(dialog).toBeHidden();
  await expect(
    page.getByRole('button', { name: 'Settings', exact: true }),
  ).toBeFocused();
  expect(backend.writes).toHaveLength(0);
});

test('Apply appears for changed settings and identifies the save destination', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  await page.goto('/');
  const { dialog } = await openSettings(page);
  const apply = dialog.getByRole('button', { name: 'Apply', exact: true });
  const name = dialog.getByLabel('Player name', { exact: true });
  await expect(apply).toBeHidden();
  await name.press('Enter');
  await expect(dialog).toBeVisible();
  await name.fill('Edited player');
  await expect(apply).toBeEnabled();
  await expect(apply).toHaveAccessibleDescription(
    'Saves changes to the current configuration.',
  );
  await name.fill(initialSettings.player_name);
  await expect(apply).toBeHidden();
  await dialog.getByRole('button', { name: 'Engines', exact: true }).click();
  const path = dialog.getByLabel('GNUbg executable path', { exact: true });
  await path.fill('/opt/other/bin/gnubg');
  await expect(apply).toHaveAccessibleDescription(
    'Engine paths and thread limits are saved on this computer.',
  );
  await dialog.getByRole('button', { name: 'General', exact: true }).click();
  await name.fill('Edited player');
  await expect(apply).toHaveAccessibleDescription(
    'Saves changes to the current configuration. Engine paths and thread limits are saved on this computer.',
  );
  await name.fill(initialSettings.player_name);
  await dialog.getByRole('button', { name: 'Engines', exact: true }).click();
  await path.fill(initialSettings.gnubg_path);
  await expect(apply).toBeHidden();
  expect(backend.writes).toHaveLength(0);
});

test('analysis keeps profile settings and Engines provides a dedicated local configuration section', async ({
  page,
}, info) => {
  await settingsBackend(page);
  await page.setViewportSize({ width: 1800, height: 1169 });
  await page.goto('/');
  const { dialog } = await openSettings(page);
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  const engine = dialog.getByLabel('Analysis engine', { exact: true });
  const depth = dialog.getByLabel('Default analysis profile', { exact: true });
  await expect(dialog.getByText('Automatic', { exact: true })).toBeVisible();
  const path = dialog.getByLabel('GNUbg executable path', { exact: true });
  await expect(path).toBeHidden();
  for (const { width, height } of DESKTOP_VIEWPORTS) {
    await page.setViewportSize({ width, height });
    const engineBox = await engine.boundingBox();
    const depthBox = await depth.boundingBox();
    expect(engineBox!.y).toBe(depthBox!.y);
    expect(engineBox!.x).toBeLessThan(depthBox!.x);
    await dialog.screenshot({ path: info.outputPath(`analysis-${width}.png`) });
  }
  await dialog.getByRole('button', { name: 'Engines', exact: true }).click();
  await expect(
    dialog.getByText('Engines on this computer', { exact: true }),
  ).toBeVisible();
  await expect(
    dialog.getByText('GNUbg · Available', { exact: true }),
  ).toBeVisible();
  await expect(
    dialog.getByText('NeonGammon · Not available', { exact: true }),
  ).toBeVisible();
  await expect(path).toBeVisible();
  for (const { width, height } of DESKTOP_VIEWPORTS) {
    await page.setViewportSize({ width, height });
    await expect(path).toBeInViewport();
    await expect(
      dialog.getByRole('combobox', { name: 'Simultaneous analysis jobs' }),
    ).toBeInViewport({ ratio: 1 });
    await dialog.screenshot({ path: info.outputPath(`engines-${width}.png`) });
  }
});

test('playing groups engine, feedback and game preferences without desktop scrolling', async ({
  page,
}, info) => {
  await settingsBackend(page);
  await page.goto('/');
  const { dialog } = await openSettings(page);
  await dialog.getByRole('button', { name: 'Playing', exact: true }).click();
  const engine = dialog.getByLabel('Engine for future games', { exact: true });
  const strength = dialog.getByLabel('Strength for future games', {
    exact: true,
  });
  const feedback = dialog.getByRole('group', {
    name: 'Play mode and feedback',
    exact: true,
  });
  const preferences = dialog.getByRole('group', {
    name: 'Sounds and board direction',
    exact: true,
  });

  for (const { width, height } of DESKTOP_VIEWPORTS) {
    await page.setViewportSize({ width, height });
    const engineBox = (await engine.boundingBox())!;
    const strengthBox = (await strength.boundingBox())!;
    expect(engineBox.y).toBe(strengthBox.y);
    expect(engineBox.x + engineBox.width).toBeLessThan(strengthBox.x);
    await expect(
      feedback.getByLabel('Default play mode', { exact: true }),
    ).toBeInViewport({ ratio: 1 });
    await expect(
      feedback.getByLabel('Open training analysis automatically', {
        exact: true,
      }),
    ).toBeInViewport({ ratio: 1 });
    const feedbackBox = (await feedback.boundingBox())!;
    const preferencesBox = (await preferences.boundingBox())!;
    expect(feedbackBox.y).toBeGreaterThan(strengthBox.y + strengthBox.height);
    expect(preferencesBox.y).toBeGreaterThan(
      feedbackBox.y + feedbackBox.height,
    );
    await expect(
      preferences.getByLabel('Game sounds', { exact: true }),
    ).toBeInViewport({ ratio: 1 });
    await expect(
      preferences.getByLabel('Bear-off direction', { exact: true }),
    ).toBeInViewport({ ratio: 1 });
    await dialog.screenshot({ path: info.outputPath(`playing-${width}.png`) });
  }
});

for (const { version, supported, onePly, verified } of [
  {
    version: '3.1.2',
    supported: [1, 2, 3, 4],
    onePly: true,
    verified: '1-ply, 2-ply',
  },
  { version: '3.0.0', supported: [2, 3], onePly: undefined, verified: '2-ply' },
]) {
  test(`NeonGammon ${version} shows supported and verified depths independently`, async ({
    page,
  }, info) => {
    await settingsBackend(page);
    const python = `/Volumes/Engine installations/NeonDojo/local-data/neongammon-${version}/venv/bin/python`;
    let savedPath = python;
    let available = true;
    const catalogPending = Promise.withResolvers<void>();
    await page.route('**/api/engines/bgsage', (route) => {
      if (route.request().method() === 'PUT') {
        if (!available)
          return fulfillJson<'ErrorBody'>(
            route,
            { error: 'Python not found' },
            422,
          );
        const request: components['schemas']['InstallationRequest'] = route
          .request()
          .postDataJSON();
        savedPath = request.path;
      }
      return fulfillJson<'Installation'>(route, {
        version: 7,
        python_path: savedPath,
        snapshot: {
          python: savedPath,
          package_version: version,
          plies: 2,
          threads: 4,
          adapter_version: 1,
          artifact_fingerprint: 'a'.repeat(64),
          one_ply_verified: savedPath === python ? onePly : false,
        },
      });
    });
    await mockEngineCatalog(page, async () => {
      if (savedPath !== python) await catalogPending.promise;
      return [
        {
          id: 'gnubg',
          name: 'GNUbg',
          available: true,
          version: 'GNU Backgammon 1.08.003',
          max_parallel_threads: 14,
          plies: [1, 2, 3, 4, 5],
        },
        {
          id: 'bgsage',
          name: 'NeonGammon',
          available,
          version: available
            ? `NeonGammon ${version}; model ${'a'.repeat(64)}; 2ply cubeful; threads 4; adapter 1`
            : null,
          max_parallel_threads: 14,
          plies: savedPath === python ? supported : [2, 3, 4],
        },
      ];
    });
    await page.goto('/');
    const { dialog } = await openSettings(page);
    await dialog.getByRole('button', { name: 'Engines', exact: true }).click();
    const card = dialog.getByRole('group', { name: 'NeonGammon', exact: true });
    await expect(
      card.getByText(`Detected version: NeonGammon ${version}`, {
        exact: true,
      }),
    ).toBeVisible();
    await expect(
      card.getByText(`Last verified: ${verified}`, { exact: true }),
    ).toBeVisible();
    await expect(
      card.getByText(
        `Supported analysis depths: ${supported.map((ply) => `${ply}-ply`).join(', ')}`,
        { exact: true },
      ),
    ).toBeVisible();
    for (const [width, height] of [
      [1800, 1169],
      [2560, 1440],
    ]) {
      await page.setViewportSize({ width, height });
      await expect(
        dialog.getByRole('combobox', { name: 'Simultaneous analysis jobs' }),
      ).toBeInViewport({ ratio: 1 });
      await expect(
        card.getByRole('textbox', { name: 'Python path' }),
      ).toHaveAttribute('title', python);
      for (const fieldset of await dialog.locator('.settings-engine').all()) {
        const field = await fieldset.getByRole('textbox').boundingBox();
        const button = await fieldset.getByRole('button').first().boundingBox();
        expect(button!.y).toBeGreaterThanOrEqual(field!.y + field!.height);
        const status = await fieldset
          .locator('.settings-engine-status')
          .boundingBox();
        const help = await fieldset.locator('p.settings-help').boundingBox();
        expect(help!.y).toBeGreaterThanOrEqual(status!.y + status!.height);
      }
      await dialog.screenshot({
        path: info.outputPath(`verified-engines-${width}.png`),
      });
    }
    await card
      .getByRole('textbox', { name: 'Python path' })
      .fill('/another/python');
    await expect(card.getByText('Installation not verified')).toBeVisible();
    await expect(
      card.getByText(
        /Detected version|Supported analysis depths|Last verified:/,
      ),
    ).toHaveCount(0);
    const refreshing = page.waitForRequest(
      (request) => new URL(request.url()).pathname === '/api/engines',
    );
    await card.getByRole('button', { name: 'Verify and save' }).click();
    await refreshing;
    try {
      await expect(
        card.getByText('Last verified: 2-ply', { exact: true }),
      ).toBeVisible();
      await expect(card.getByText(/Supported analysis depths/)).toHaveCount(0);
    } finally {
      catalogPending.resolve();
    }
    await expect(
      card.getByText('Supported analysis depths: 2-ply, 3-ply, 4-ply'),
    ).toBeVisible();
    available = false;
    await card.getByRole('button', { name: 'Verify and save' }).click();
    await expect(card.getByRole('alert')).toContainText('Python not found');
    await expect(
      card.getByText('Not available', { exact: true }),
    ).toBeVisible();
    await dialog.getByRole('button', { name: 'Close', exact: true }).click();
    await openSettings(page);
    await dialog.getByRole('button', { name: 'Engines', exact: true }).click();
    await expect(
      card.getByText('Not available', { exact: true }),
    ).toBeVisible();
    await expect(
      card.getByText(`Last detected version: NeonGammon ${version}`),
    ).toBeVisible();
    await expect(card.getByText('Last verified: 2-ply')).toBeVisible();
    await expect(card.getByText(/Supported analysis depths/)).toHaveCount(0);
  });
}

test('an edited NeonGammon path stays in the draft until verified or discarded', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  const original = '/opt/sage/bin/python';
  const edited = '/opt/new-sage/bin/python';
  let attempts = 0;
  await page.route('**/api/engines/bgsage', (route) => {
    if (route.request().method() === 'GET') {
      return fulfillJson<'Installation'>(route, {
        version: 2,
        python_path: original,
      });
    }
    attempts++;
    return fulfillJson<'ErrorBody'>(
      route,
      { error: 'NeonGammon probe failed' },
      422,
    );
  });
  await page.goto('/');
  const { dialog } = await openSettings(page);
  await dialog.getByRole('button', { name: 'Engines', exact: true }).click();
  const path = dialog.getByLabel('Python path', { exact: true });
  await expect(path).toHaveValue(original);
  await expect(
    dialog.getByRole('button', { name: 'Verify and save', exact: true }),
  ).toHaveAccessibleDescription(
    'Verifies and saves the Python path on this computer.',
  );
  await path.fill(edited);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  const guard = page.getByRole('dialog', { name: 'Unapplied settings' });
  await expect(
    guard.getByRole('button', { name: 'Apply', exact: true }),
  ).toHaveAccessibleDescription(
    'Engine paths and thread limits are saved on this computer.',
  );
  await guard.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(guard.getByRole('alert')).toContainText(
    'NeonGammon probe failed',
  );
  await guard
    .getByRole('button', { name: 'Keep editing', exact: true })
    .click();
  await expect(path).toHaveValue(edited);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await guard
    .getByRole('button', { name: 'Discard draft', exact: true })
    .click();
  await expect(dialog).toBeHidden();
  expect(attempts).toBe(1);
  expect(backend.writes).toHaveLength(0);
});

test('benchmark waits for the settings draft to be applied successfully', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  await page.goto('/');
  const { dialog } = await openSettings(page);
  await dialog
    .getByLabel('Player name', { exact: true })
    .fill('Benchmark draft');
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  const benchmark = dialog.getByRole('button', {
    name: 'Open benchmark',
    exact: true,
  });
  await benchmark.click();
  const guard = page.getByRole('dialog', { name: 'Unapplied settings' });
  await expect(guard).toBeVisible();
  await guard
    .getByRole('button', { name: 'Keep editing', exact: true })
    .click();
  await expect(benchmark).toBeFocused();
  expect(backend.writes).toHaveLength(0);

  backend.conflict(true);
  await benchmark.click();
  await guard
    .getByRole('button', { name: 'Apply and continue', exact: true })
    .click();
  await expect(guard.getByRole('alert')).toContainText(
    'Settings changed elsewhere',
  );
  await expect(guard).toBeVisible();
  await guard
    .getByRole('button', { name: 'Keep editing', exact: true })
    .click();
  await dialog.getByRole('button', { name: 'General', exact: true }).click();
  await expect(dialog.getByLabel('Player name', { exact: true })).toHaveValue(
    'Benchmark draft',
  );
  backend.conflict(false);
  await dialog
    .getByRole('button', { name: 'Load latest and discard draft', exact: true })
    .click();
  await expect(dialog.getByLabel('Player name', { exact: true })).toHaveValue(
    initialSettings.player_name,
  );
  await dialog
    .getByLabel('Player name', { exact: true })
    .fill('Apply before benchmark');
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await benchmark.click();
  await guard
    .getByRole('button', { name: 'Apply and continue', exact: true })
    .click();
  await expect(guard).toBeHidden();
  await expect(dialog).toBeHidden();
  await expect(
    page.getByRole('tab', { name: 'Benchmark', exact: true }),
  ).toHaveAttribute('aria-selected', 'true');
  expect(backend.writes.at(-1)?.player_name).toBe('Apply before benchmark');
});

test('saves separate engine thread limits and restores them when reopened', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  await page.goto('/');
  const { dialog } = await openSettings(page);
  await dialog.getByRole('button', { name: 'Engines', exact: true }).click();
  const fields = [
    ['GNUbg threads for analysis', 1, 7, 1, '3'],
    ['GNUbg threads for playing', 1, 7, 1, '2'],
    ['NeonGammon threads for analysis', 2, 9, 4, '6'],
    ['NeonGammon threads for playing', 2, 9, 2, '4'],
  ] as const;
  for (const [label, min, max, defaultThreads] of fields) {
    const field = dialog.getByRole('combobox', { name: label, exact: true });
    await expect(field).toHaveAccessibleDescription(
      new RegExp(`Configurable range ${min}–${max}`),
    );
    await expect(field.locator('option').first()).toHaveText(
      `Default (${defaultThreads})`,
    );
    await expect(field.locator('option').last()).toHaveAttribute(
      'value',
      String(max),
    );
  }
  for (const [label, , , , value] of fields)
    await dialog
      .getByRole('combobox', { name: label, exact: true })
      .selectOption(value);
  const parallelJobs = dialog.getByRole('combobox', {
    name: 'Simultaneous analysis jobs',
    exact: true,
  });
  await expect(parallelJobs).toHaveAccessibleDescription(
    /NeonGammon runs one analysis at a time/,
  );
  await expect(parallelJobs.locator('option').first()).toHaveText(
    'Default (1)',
  );
  await expect(parallelJobs.locator('option').last()).toHaveAttribute(
    'value',
    '8',
  );
  await parallelJobs.selectOption('3');
  expect(backend.writes).toHaveLength(0);
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(
    dialog.getByRole('combobox', {
      name: 'GNUbg threads for analysis',
      exact: true,
    }),
  ).toHaveValue('3');
  await expect(
    dialog.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeHidden();
  await expect(dialog).toBeVisible();
  await expect(
    dialog.getByRole('button', { name: 'Engines', exact: true }),
  ).toHaveAttribute('aria-current', 'page');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(backend.writes).toEqual([
    expect.objectContaining({
      gnubg_threads: 3,
      gnubg_play_threads: 2,
      open_sage_threads: 6,
      open_sage_play_threads: 4,
      parallel_analysis_jobs: 3,
    }),
  ]);
  await openSettings(page);
  await dialog.getByRole('button', { name: 'Engines', exact: true }).click();
  for (const [label, , , , value] of fields)
    await expect(
      dialog.getByRole('combobox', { name: label, exact: true }),
    ).toHaveValue(value);
  await expect(
    dialog.getByRole('combobox', {
      name: 'Simultaneous analysis jobs',
      exact: true,
    }),
  ).toHaveValue('3');
});

test('English default, alphabetical languages, draft cancellation, focus trap and restore', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  const { dialog, trigger } = await openSettings(page);
  const languages = dialog.getByLabel('Language', { exact: true });
  expect(await languages.locator('option').allTextContents()).toEqual([
    'Deutsch',
    'English',
  ]);
  await languages.selectOption('de');
  await dialog.getByLabel('Player name', { exact: true }).fill('Draft');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await dialog.getByRole('button', { name: 'Board', exact: true }).click();
  await expect(dialog.locator('.settings-theme-choice')).toHaveCount(
    themes.length,
  );
  await dialog.getByRole('button', { name: 'Walnut', exact: true }).click();
  await expect(
    dialog.locator('.settings-preview .board-shell.theme-walnut'),
  ).toBeVisible();
  await dialog.getByRole('button', { name: 'Apply', exact: true }).focus();
  await page.keyboard.press('Tab');
  await expect(
    dialog.getByRole('button', { name: 'Close settings' }),
  ).toBeFocused();
  await page.keyboard.press('Escape');
  const guard = page.getByRole('dialog', { name: 'Unapplied settings' });
  await expect(guard).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(guard).toBeHidden();
  await expect(
    dialog.locator('.settings-preview .board-shell.theme-walnut'),
  ).toBeVisible();
  await dialog
    .getByRole('button', { name: 'Close settings', exact: true })
    .click();
  await expect(guard).toBeVisible();
  await guard
    .getByRole('button', { name: 'Discard draft', exact: true })
    .click();
  await expect(dialog).not.toBeVisible();
  await expect(trigger).toBeFocused();
  expect(backend.writes).toHaveLength(0);
  expect(
    await page.evaluate(() => localStorage.getItem('neondojo.theme')),
  ).not.toBe('walnut');
  await openSettings(page);
  await expect(page.getByLabel('Language', { exact: true })).toHaveValue('en');
  await expect(page.getByLabel('Player name', { exact: true })).toHaveValue(
    initialSettings.player_name,
  );
});

test('board, checker and cube choices preview independently and survive reload after Apply', async ({
  page,
}) => {
  await settingsBackend(page);
  await page.goto('/');
  const { dialog } = await openSettings(page);
  const boardTab = dialog.getByRole('button', { name: 'Board', exact: true });
  await boardTab.click();
  const checkers = dialog.getByRole('combobox', {
    name: 'Checker set',
    exact: true,
  });
  const cube = dialog.getByRole('combobox', {
    name: 'Doubling cube',
    exact: true,
  });
  const preview = dialog.locator('.settings-preview .board-shell');
  const expectedOptions = ['Match board', ...themes.map(({ name }) => name)];
  await expect(checkers.locator('option')).toHaveText(expectedOptions);
  await expect(cube.locator('option')).toHaveText(expectedOptions);
  await expect(checkers).toHaveValue('');
  await expect(cube).toHaveValue('');

  const cubeId =
    themes.find(({ id }) => id.startsWith('bgblitz:'))?.id ?? 'carbon';
  await checkers.selectOption('ion');
  await cube.selectOption(cubeId);
  await dialog.getByRole('button', { name: 'Walnut', exact: true }).click();
  await expect(preview).toHaveAttribute('data-checker-theme', 'ion');
  await expect(preview.locator('.play-cube-artwork')).toHaveAttribute(
    'data-cube-theme',
    cubeId,
  );
  await applyAndCloseSettings(dialog);
  await page.reload();
  await openSettings(page);
  await boardTab.click();
  await expect(checkers).toHaveValue('ion');
  await expect(cube).toHaveValue(cubeId);
  const walnut = dialog.getByRole('button', { name: 'Walnut', exact: true });
  await expect(walnut).toHaveAttribute('aria-pressed', 'true');
  await expect(walnut.locator('.board-shell')).toHaveAttribute(
    'data-checker-theme',
    'walnut',
  );

  await cube.selectOption('');
  await expect(preview.locator('.play-cube-artwork')).toHaveAttribute(
    'data-cube-theme',
    'walnut',
  );
  await page.keyboard.press('Escape');
  await page
    .getByRole('button', { name: 'Discard draft', exact: true })
    .click();
  await openSettings(page);
  await boardTab.click();
  await expect(cube).toHaveValue(cubeId);
  await expect(checkers).toHaveValue('ion');
});

test('real artwork previews, independent profiles and settings persist only after save', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  await page.goto('/');
  const { dialog } = await openSettings(page);
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await dialog
    .getByLabel('Default analysis profile', { exact: true })
    .selectOption('3');
  await expect(
    dialog
      .getByLabel('Analysis engine', { exact: true })
      .locator('option[value=bgsage]'),
  ).toBeDisabled();
  await dialog.getByRole('button', { name: 'Playing', exact: true }).click();
  await expect(
    dialog.getByLabel('Strength for future games', { exact: true }),
  ).toHaveValue('2');
  await dialog.getByRole('button', { name: 'Board', exact: true }).click();
  const archive = themes.find(({ id }) => id.startsWith('bgblitz:'));
  if (archive) {
    await dialog
      .getByRole('button', { name: archive.name, exact: true })
      .click();
    const preview = dialog.locator('.settings-preview .bgblitz-board');
    await expect(preview).toHaveAttribute('data-theme', archive.id);
    await expect(preview.locator('image').first()).toHaveAttribute(
      'href',
      /\/themes\//,
    );
    await expect(preview.locator('[data-dice]')).toHaveAttribute(
      'data-dice',
      '3-2',
    );
    await expect(preview.locator('[data-point="bar-white"]')).toHaveAttribute(
      'data-count',
      '1',
    );
  }
  await applyAndCloseSettings(dialog);
  expect(backend.writes).toHaveLength(1);
  expect(backend.writes[0]).toMatchObject({
    version: 3,
    analysis_plies: 3,
    play_plies: 2,
  });
  await openSettings(page);
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Analysis', exact: true })
    .click();
  await expect(
    page.getByLabel('Default analysis profile', { exact: true }),
  ).toHaveValue('3');
});

test('installation failures and save conflicts retain drafts and do not apply preferences', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  await page.goto('/');
  const { dialog } = await openSettings(page);
  await dialog
    .getByLabel('Player name', { exact: true })
    .fill('Retained draft');
  await dialog.getByLabel('Language', { exact: true }).selectOption('de');
  await dialog.getByRole('button', { name: 'Engines', exact: true }).click();
  await dialog
    .getByLabel('GNUbg executable path', { exact: true })
    .fill('/missing/gnubg');
  await dialog
    .getByRole('button', { name: 'Check installation', exact: true })
    .click();
  await expect(dialog.getByRole('alert')).toContainText('Executable not found');
  await expect(
    dialog.getByLabel('GNUbg executable path', { exact: true }),
  ).toHaveValue('/missing/gnubg');
  backend.conflict(true);
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(
    dialog.getByRole('alert').filter({ hasText: 'Settings changed elsewhere' }),
  ).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await dialog.getByRole('button', { name: 'General', exact: true }).click();
  await expect(dialog.getByLabel('Player name', { exact: true })).toHaveValue(
    'Retained draft',
  );
  await expect(dialog.getByLabel('Language', { exact: true })).toHaveValue(
    'de',
  );
  await dialog
    .getByRole('button', { name: 'Load latest and discard draft', exact: true })
    .click();
  await expect(dialog.getByLabel('Player name', { exact: true })).toHaveValue(
    initialSettings.player_name,
  );
});

test('load errors offer retry and the mobile dialog has reachable actions', async ({
  page,
}) => {
  await settingsBackend(page);
  let unavailable = true;
  await mockSettings(page, ({ respond, fail }) =>
    unavailable
      ? fail(503, { error: 'Temporarily unavailable' })
      : respond(initialSettings),
  );
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await expect(dialog.getByRole('alert')).toContainText(
    'Settings could not be loaded',
  );
  await expect(
    dialog.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeHidden();
  await expect(dialog.getByLabel('Player name', { exact: true })).toHaveCount(
    0,
  );
  unavailable = false;
  await dialog.getByRole('button', { name: 'Try again', exact: true }).click();
  await expect(dialog.getByLabel('Player name', { exact: true })).toHaveValue(
    initialSettings.player_name,
  );
  await dialog.getByRole('button', { name: 'Board', exact: true }).click();
  await expect(
    dialog.getByRole('button', { name: 'Close', exact: true }),
  ).toBeInViewport();
  await expect(
    dialog.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeHidden();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});

for (const plies of [4, 5]) {
  test(`GNUbg 3-ply defaults and ${plies}-ply selections persist through settings and submit real analysis`, async ({
    page,
  }) => {
    test.setTimeout(300000);
    page.setDefaultTimeout(10000);
    const backend = await settingsBackend(page, 3);
    await page.goto('/');
    await page.getByLabel('Match file', { exact: true }).setInputFiles({
      name: 'depth.sgf',
      mimeType: 'application/octet-stream',
      // Both players are bearing off, with multiple legal ways to play 3–1.
      buffer: Buffer.from(
        `(;GM[6]PW[Depth ${crypto.randomUUID()}]PB[Opponent]MI[length:7]AE[a:y]AW[d][e][f]AB[s][t][u]PL[W];W[31fccb])`,
      ),
    });
    const initialPositionCalculation = await openPositionCalculation(page);
    await expect(
      initialPositionCalculation.getByLabel('Search depth', { exact: true }),
    ).toHaveValue('3');
    await page.keyboard.press('Escape');
    await openMatchAnalysisOptions(page);
    await expect(
      page.getByLabel('Match analysis depth', { exact: true }),
    ).toHaveValue('3');
    const { dialog } = await openSettings(page);
    await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
    const profile = dialog.getByLabel('Default analysis profile', {
      exact: true,
    });
    await expect(profile).toHaveValue('3');
    await profile.selectOption(String(plies));
    await applyAndCloseSettings(dialog);
    expect(backend.writes.at(-1)?.analysis_plies).toBe(plies);
    await openMatchAnalysisOptions(page);
    await expect(
      page.getByLabel('Match analysis depth', { exact: true }),
    ).toHaveValue(String(plies));
    const positionCalculation = await openPositionCalculation(page);
    await expect(
      positionCalculation.getByLabel('Search depth', { exact: true }),
    ).toHaveValue(String(plies));
    const submitted = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === '/api/jobs' &&
        response.request().method() === 'POST',
    );
    await positionCalculation
      .getByRole('button', { name: 'Analyze position', exact: true })
      .click();
    const response = await submitted;
    expect(response.status()).toBe(202);
    expect(response.request().postDataJSON().plies).toBe(plies);
    const job = await response.json();
    await waitForJobCompletion(page, job.id, 90000);
    await expect(
      page.locator('.analysis-panel .candidates tbody tr').first(),
    ).toBeVisible();
    if (plies === 5) {
      for (const scope of ['dice', 'game', 'match']) {
        const submitted = page.waitForResponse(
          (response) =>
            response.request().method() === 'POST' &&
            (scope === 'dice'
              ? new URL(response.url()).pathname === '/api/jobs'
              : /\/api\/matches\/[^/]+\/analysis$/.test(
                  new URL(response.url()).pathname,
                )),
        );
        if (scope === 'dice') await calculateDiceEquity(page);
        else if (scope === 'match')
          await submitFullMatchAnalysis(
            page,
            await openFullMatchAnalysis(page),
          );
        else
          await page
            .getByRole('button', { name: 'Analyze game', exact: true })
            .click();
        const response = await submitted;
        expect(response.status()).toBe(202);
        const request = response.request().postDataJSON();
        expect(request.plies).toBe(5);
        if (scope === 'dice') expect(request.kind).toBe('dice');
        else expect(request.game_no).toBe(scope === 'game' ? 1 : undefined);
        const job = await response.json();
        await expect
          .poll(
            async () =>
              (await (await page.request.get(`/api/jobs/${job.id}`)).json())
                .status,
            { timeout: 180000 },
          )
          .toBe('completed');
        const completed = await (
          await page.request.get(`/api/jobs/${job.id}`)
        ).json();
        expect(completed.result.plies).toBe(5);
        if (scope === 'dice') expect(completed.result.rolls).toHaveLength(21);
        else expect(completed.completed_games).toBe(1);
      }
    }
    await page.reload();
    await expect(
      (await openPositionCalculation(page)).getByLabel('Search depth', {
        exact: true,
      }),
    ).toHaveValue(String(plies));
  });
}

backendTest(
  'the header saves a modified profile before switching and keeps notices in one badge',
  async ({ page }, testInfo) => {
    const backend = await profileBackend(page);
    await page.route('**/api/configuration/availability', (route) =>
      route.fulfill({
        json: {
          revision: backend.state.revision,
          valid: false,
          issues: [
            {
              path: 'opening_book.path',
              code: 'unavailable',
              message: 'The opening book file is missing on this computer.',
            },
            {
              path: 'opening_book.xg_path',
              code: 'unavailable',
              message: 'The XG opening book file is missing on this computer.',
            },
          ],
        },
      }),
    );
    await page.setViewportSize({ width: 1800, height: 1169 });
    await page.goto('/');
    const header = page.locator('.app-header');
    const notices = header.getByRole('group', {
      name: 'Profile notices: 2',
    });
    const badge = header.locator('.profile-notices > summary');
    await expect(badge).toHaveAccessibleName('Profile notices: 2');
    await expect(notices).toBeHidden();
    // The header stays one row: the badge sits beside the profile trigger.
    const trigger = page.getByRole('button', { name: /Profile: Standard/ });
    const tabs = header.getByRole('tablist').first();
    const [triggerBox, badgeBox, tabsBox] = await Promise.all([
      trigger.boundingBox(),
      badge.boundingBox(),
      tabs.boundingBox(),
    ]);
    expect(Math.abs(badgeBox!.y - triggerBox!.y)).toBeLessThan(
      triggerBox!.height,
    );
    expect(tabsBox!.y).toBeLessThan(triggerBox!.y + triggerBox!.height);
    expect((await header.boundingBox())!.height).toBeLessThanOrEqual(80);
    await badge.focus();
    await page.keyboard.press('Enter');
    await expect(notices).toBeVisible();
    await expect(notices).toContainText(
      'Opening book · BGDB book file: The opening book file is missing on this computer.',
    );
    await page.screenshot({ path: testInfo.outputPath('header-notices.png') });
    await page.keyboard.press('Escape');
    await expect(notices).toBeHidden();
    await expect(badge).toBeFocused();

    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await page
      .getByLabel('Player name', { exact: true })
      .fill('Saved from the header');
    await applyAndCloseSettings(
      page.getByRole('dialog', { name: 'Settings', exact: true }),
    );
    await expect(trigger).toContainText('modified');
    await trigger.click();
    await page
      .getByRole('menuitemradio', { name: 'Deep analysis', exact: true })
      .click();
    const switchDialog = page.getByRole('dialog', {
      name: 'Switch profile',
      exact: true,
    });
    const save = switchDialog.getByRole('button', {
      name: 'Save and switch',
      exact: true,
    });
    await expect(save).toHaveClass(/primary/);
    await save.click();
    await expect(switchDialog).toBeHidden();
    await expect(
      page.getByRole('button', { name: /Profile: Deep analysis/ }),
    ).not.toContainText('modified');
    expect(backend.profiles[0].configuration.general.player_name).toBe(
      'Saved from the header',
    );
    expect(
      backend.calls.some((call) => call.body?.source_action === 'save_current'),
    ).toBe(true);
  },
);

test('discarding a settings draft resets error thresholds and every other draft field, and profile list uses listitem roles', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  await page.goto('/');
  const { dialog } = await openSettings(page);

  const playerName = dialog.getByLabel('Player name', { exact: true });
  const initialPlayerName = await playerName.inputValue();
  const language = dialog.getByLabel('Language', { exact: true });
  const initialLanguage = await language.inputValue();

  await playerName.fill('Unsaved Player');
  await language.selectOption('de');

  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  const questionable = dialog.getByLabel('Questionable', { exact: true });
  const blunder = dialog.getByLabel('Blunder', { exact: true });
  const initialQuestionable = await questionable.inputValue();
  const initialBlunder = await blunder.inputValue();
  await questionable.fill('0.031');
  await blunder.fill('0.125');

  const applyButton = dialog.getByRole('button', {
    name: 'Apply',
    exact: true,
  });
  await expect(applyButton).toBeEnabled();

  await dialog
    .getByRole('button', { name: 'Manage opening books', exact: true })
    .click();
  const guard = page.getByRole('dialog', { name: 'Unapplied settings' });
  await expect(guard).toBeVisible();
  await guard
    .getByRole('button', { name: 'Discard draft', exact: true })
    .click();

  const bookDialog = page.getByRole('dialog', { name: 'Opening books' });
  await expect(bookDialog).toBeVisible();
  await bookDialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(bookDialog).toBeHidden();

  await expect(questionable).toHaveValue(initialQuestionable);
  await expect(blunder).toHaveValue(initialBlunder);
  await expect(applyButton).toBeHidden();

  await dialog.getByRole('button', { name: 'General', exact: true }).click();
  await expect(playerName).toHaveValue(initialPlayerName);
  await expect(language).toHaveValue(initialLanguage);

  // Profiles list wraps every item in role="listitem".
  await dialog.getByRole('button', { name: 'Profiles', exact: true }).click();
  const profileList = dialog.getByRole('list', {
    name: 'Profiles',
    exact: true,
  });
  await expect(profileList).toBeVisible();
  await expect(profileList.locator(':scope > button')).toHaveCount(0);
  await expect(
    profileList.locator(':scope > [role="listitem"] > button'),
  ).toHaveCount(1);

  // Closing Settings now succeeds immediately without re-opening the draft guard.
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(dialog).toBeHidden();
  await expect(guard).toBeHidden();
  expect(backend.writes).toHaveLength(0);
});
