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
          plies: [0, 1, 2, 3, 4],
        },
        {
          id: 'bgsage',
          name: 'BGSage',
          available: false,
          version: null,
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

test('saves separate engine thread limits and restores them when reopened', async ({
  page,
}) => {
  const backend = await settingsBackend(page);
  await page.goto('/');
  const { dialog } = await openSettings(page);
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
  await dialog
    .getByRole('combobox', { name: 'GNUbg threads', exact: true })
    .selectOption('3');
  await dialog
    .getByRole('combobox', { name: 'Open Sage threads', exact: true })
    .selectOption('6');
  expect(backend.writes).toHaveLength(0);
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(backend.writes).toContainEqual(
    expect.objectContaining({
      gnubg_threads: 3,
      open_sage_threads: 6,
    }),
  );
  await openSettings(page);
  await dialog.getByRole('button', { name: 'Analysis', exact: true }).click();
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
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).not.toBeVisible();
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
  ).toBeDisabled();
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
    dialog.getByRole('button', { name: 'Cancel', exact: true }),
  ).toBeInViewport();
  await expect(
    dialog.getByRole('button', { name: 'Apply', exact: true }),
  ).toBeInViewport();
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
    await expect(page.getByLabel('Search depth', { exact: true })).toHaveValue(
      '2',
    );
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
    await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
    await expect(dialog).not.toBeVisible();
    expect(backend.writes.at(-1)?.analysis_plies).toBe(plies);
    await expect(page.getByLabel('Search depth', { exact: true })).toHaveValue(
      String(plies),
    );
    await expect(
      page.getByLabel('Match analysis depth', { exact: true }),
    ).toHaveValue(String(plies));
    const submitted = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === '/api/jobs' &&
        response.request().method() === 'POST',
    );
    await page
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
        if (scope !== 'dice')
          await page
            .getByLabel('Analysis scope', { exact: true })
            .selectOption(scope);
        const submitted = page.waitForResponse(
          (response) =>
            response.request().method() === 'POST' &&
            (scope === 'dice'
              ? new URL(response.url()).pathname === '/api/jobs'
              : /\/api\/matches\/[^/]+\/analysis$/.test(
                  new URL(response.url()).pathname,
                )),
        );
        await page
          .getByRole('button', {
            name:
              scope === 'dice'
                ? 'Dice distribution'
                : scope === 'game'
                  ? 'Analyze game'
                  : 'Analyze entire match',
            exact: true,
          })
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
    await expect(page.getByLabel('Search depth', { exact: true })).toHaveValue(
      String(plies),
    );
  });
}
