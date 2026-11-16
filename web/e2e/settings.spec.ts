import { applyAndCloseSettings } from './settings-actions';
import {
  calculateDiceEquity,
  openFullMatchAnalysis,
  openMatchAnalysisOptions,
  openPositionCalculation,
  submitFullMatchAnalysis,
} from './analysis-controls';
import { test, expect, type Page } from './test';
import { readFileSync } from 'node:fs';

const themes: { id: string; name: string }[] = [
  { id: 'forest', name: 'Forest' },
  { id: 'walnut', name: 'Walnut' },
  { id: 'slate', name: 'Slate' },
  ...JSON.parse(
    readFileSync(
      new URL('../src/generated-themes.json', import.meta.url),
      'utf8',
    ),
  ),
];

const initialSettings = {
  version: 3,
  player_name: 'Settings Player',
  analysis_engine: 'gnubg',
  analysis_plies: 0,
  play_engine: 'gnubg',
  play_plies: 1,
  gnubg_path: '/opt/bin/gnubg',
};

async function settingsBackend(
  page: Page,
  analysisPlies = initialSettings.analysis_plies,
) {
  let saved = { ...initialSettings, analysis_plies: analysisPlies };
  let conflict = false;
  const writes: (typeof initialSettings)[] = [];
  await page.route('**/api/settings', async (route) => {
    if (route.request().method() === 'GET')
      return route.fulfill({ json: saved });
    const body = route.request().postDataJSON();
    writes.push(body);
    if (conflict)
      return route.fulfill({ status: 409, json: { error: 'Version changed' } });
    saved = { ...body, version: saved.version + 1 };
    await route.fulfill({ json: saved });
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
  await page.route('**/api/engines', (route) =>
    route.fulfill({
      json: [
        {
          id: 'gnubg',
          name: 'GNU Backgammon',
          available: true,
          version: 'GNUbg 1.08',
          max_parallel_threads: 7,
          plies: [0, 1, 2, 3, 4],
        },
        {
          id: 'bgsage',
          name: 'BGSage',
          available: false,
          version: null,
          max_parallel_threads: 9,
          plies: [],
        },
      ],
    }),
  );
  await page.route('**/api/engines/gnubg/check', (route) =>
    route.fulfill({ status: 422, json: { error: 'Executable not found' } }),
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
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await dialog.getByText('Engines on this computer', { exact: true }).click();
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
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await path.fill(initialSettings.gnubg_path);
  await expect(apply).toBeHidden();
  expect(backend.writes).toHaveLength(0);
});

test('analysis puts defaults and book source ahead of local engine setup', async ({
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
  const setup = dialog.getByText('Engines on this computer', { exact: true });
  await expect(setup).toBeInViewport();
  await expect(
    dialog.getByText('GNUbg · Available', { exact: true }),
  ).toBeVisible();
  await expect(
    dialog.getByText('Open Sage · Not available', { exact: true }),
  ).toBeVisible();
  const path = dialog.getByLabel('GNUbg executable path', { exact: true });
  await expect(path).toBeHidden();
  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
    await page.setViewportSize({ width, height });
    const engineBox = await engine.boundingBox();
    const depthBox = await depth.boundingBox();
    expect(engineBox!.y).toBe(depthBox!.y);
    expect(engineBox!.x).toBeLessThan(depthBox!.x);
    await expect(setup).toBeInViewport();
    await dialog.screenshot({ path: info.outputPath(`analysis-${width}.png`) });
  }
  await page.setViewportSize({ width: 1800, height: 1169 });
  await setup.click();
  await expect(path).toBeVisible();
  await dialog.screenshot({
    path: info.outputPath('analysis-engine-details.png'),
  });
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

  for (const [width, height] of [
    [1800, 1169],
    [2560, 1440],
    [3840, 2160],
  ]) {
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

test('an edited Open Sage path stays in the draft until verified or discarded', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  const original = '/opt/sage/bin/python';
  const edited = '/opt/new-sage/bin/python';
  let attempts = 0;
  await page.route('**/api/engines/bgsage', (route) => {
    if (route.request().method() === 'GET') {
      return route.fulfill({ json: { version: 2, python_path: original } });
    }
    attempts++;
    return route.fulfill({
      status: 422,
      json: { error: 'Open Sage probe failed' },
    });
  });
  await page.goto('/');
  const { dialog } = await openSettings(page);
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await dialog.getByText('Engines on this computer', { exact: true }).click();
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
    'Open Sage probe failed',
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
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await dialog.getByText('Engines on this computer', { exact: true }).click();
  await expect(
    dialog.getByRole('combobox', { name: 'GNUbg threads', exact: true }),
  ).toHaveAccessibleDescription(/Configurable range 1–7/);
  await expect(
    dialog.getByRole('combobox', { name: 'Open Sage threads', exact: true }),
  ).toHaveAccessibleDescription(/Configurable range 2–9/);
  for (const [label, max] of [
    ['GNUbg threads', 7],
    ['Open Sage threads', 9],
  ] as const) {
    const field = dialog.getByRole('combobox', { name: label, exact: true });
    await expect(field.locator('option').last()).toHaveAttribute(
      'value',
      String(max),
    );
  }
  await dialog
    .getByRole('combobox', { name: 'GNUbg threads', exact: true })
    .selectOption('3');
  await dialog
    .getByRole('combobox', { name: 'Open Sage threads', exact: true })
    .selectOption('6');
  expect(backend.writes).toHaveLength(0);
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(
    dialog.getByRole('combobox', { name: 'GNUbg threads', exact: true }),
  ).toHaveValue('3');
  await expect(
    dialog.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeHidden();
  await expect(dialog).toBeVisible();
  await expect(
    dialog.getByRole('button', { name: 'Analysis', exact: true }),
  ).toHaveAttribute('aria-current', 'page');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(backend.writes).toEqual([
    expect.objectContaining({
      gnubg_threads: 3,
      open_sage_threads: 6,
    }),
  ]);
  await openSettings(page);
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await dialog.getByText('Engines on this computer', { exact: true }).click();
  await expect(
    dialog.getByRole('combobox', { name: 'GNUbg threads', exact: true }),
  ).toHaveValue('3');
  await expect(
    dialog.getByRole('combobox', { name: 'Open Sage threads', exact: true }),
  ).toHaveValue('6');
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
    'Español',
    'Français',
    'Italiano',
    'Polski',
  ]);
  await languages.selectOption('fr');
  await dialog.getByLabel('Player name', { exact: true }).fill('Draft');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await dialog.getByRole('button', { name: 'Board', exact: true }).click();
  await expect(dialog.locator('.settings-theme-choice')).toHaveCount(
    themes.length,
  );
  await dialog.getByRole('button', { name: 'Walnut', exact: true }).click();
  await expect(dialog.locator('.settings-preview .theme-walnut')).toBeVisible();
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
  await expect(dialog.locator('.settings-preview .theme-walnut')).toBeVisible();
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
    await page.evaluate(() => localStorage.getItem('bgtutor.theme')),
  ).not.toBe('walnut');
  await openSettings(page);
  await expect(page.getByLabel('Language', { exact: true })).toHaveValue('en');
  await expect(page.getByLabel('Player name', { exact: true })).toHaveValue(
    initialSettings.player_name,
  );
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
    .selectOption('2');
  await expect(
    dialog
      .getByLabel('Analysis engine', { exact: true })
      .locator('option[value=bgsage]'),
  ).toBeDisabled();
  await dialog.getByRole('button', { name: 'Playing', exact: true }).click();
  await expect(
    dialog.getByLabel('Strength for future games', { exact: true }),
  ).toHaveValue('1');
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
    analysis_plies: 2,
    play_plies: 1,
  });
  await openSettings(page);
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Analysis', exact: true })
    .click();
  await expect(
    page.getByLabel('Default analysis profile', { exact: true }),
  ).toHaveValue('2');
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
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await dialog.getByText('Engines on this computer', { exact: true }).click();
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
  let fail = true;
  await page.route('**/api/settings', (route) =>
    route.fulfill(
      fail
        ? { status: 503, json: { error: 'Temporarily unavailable' } }
        : { json: initialSettings },
    ),
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
  fail = false;
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

for (const plies of [3, 4]) {
  test(`two-ply defaults and ${plies}-ply selections persist through settings and submit real analysis`, async ({
    page,
  }) => {
    test.setTimeout(300000);
    page.setDefaultTimeout(10000);
    const backend = await settingsBackend(page, 2);
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
    ).toHaveValue('2');
    await page.keyboard.press('Escape');
    await openMatchAnalysisOptions(page);
    await expect(
      page.getByLabel('Match analysis depth', { exact: true }),
    ).toHaveValue('2');
    const { dialog } = await openSettings(page);
    await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
    const profile = dialog.getByLabel('Default analysis profile', {
      exact: true,
    });
    await expect(profile).toHaveValue('2');
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
    await expect
      .poll(
        async () =>
          (await (await page.request.get(`/api/jobs/${job.id}`)).json()).status,
        { timeout: 90000 },
      )
      .toBe('completed');
    await expect(
      page.locator('.analysis-panel .candidates tbody tr').first(),
    ).toBeVisible();
    if (plies === 4) {
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
        expect(request.plies).toBe(4);
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
        expect(completed.result.plies).toBe(4);
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
