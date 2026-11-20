import {
  openFullMatchAnalysis,
  openMatchAnalysisOptions,
} from './analysis-controls';
import { expect, test, type Page } from './test';
import { profileBackend } from './profile-backend';
import { defaultConfiguration } from '../src/configuration-model';
// Keep cleanup scoped to the exact imports owned by this test, including when
// an assertion fails after import. APIRequestContext bypasses browser mocks.
const importedMatchIds = new WeakMap<Page, Set<string>>();
test.beforeEach(({ page }) => {
  importedMatchIds.set(page, new Set());
});
test.afterEach(async ({ page }, testInfo) => {
  const matchIds = [...(importedMatchIds.get(page) ?? [])];
  if (matchIds.length === 0) return;
  await testInfo.attach('imported-match-ids', {
    body: Buffer.from(JSON.stringify(matchIds)),
    contentType: 'application/json',
  });
  const response = await page.request.post('/api/library/actions', {
    data: { action: 'delete', match_ids: matchIds },
  });
  expect(response.ok(), 'delete the exact imported estimate fixtures').toBe(
    true,
  );
  const body = await response.json();
  expect(body.results).toEqual(
    matchIds.map((matchId) => ({
      match_id: matchId,
      status: 'deleted',
      created_id: null,
      existing_id: null,
      error: null,
    })),
  );
});
const estimate = (p: Page) => p.locator('.analysis-time-estimate');
function result(url: string, ms = 40_000) {
  const u = new URL(url);
  return {
    match_id: u.pathname.split('/')[3],
    game_no: u.searchParams.has('game_no')
      ? Number(u.searchParams.get('game_no'))
      : null,
    engine_id: 'gnubg',
    engine_version: 'GNU Backgammon 1.08.003',
    plies: Number(u.searchParams.get('plies')),
    position_count: 1000,
    estimated_ms: ms,
    unavailable_reason: null,
    benchmark: {
      id: 'latest',
      finished_at: '2026-11-10T12:00:00Z',
      reference_name: 'WC2014',
      average_move_ms: 40,
      protocol_id: 'v2',
    },
  };
}
async function openMatch(p: Page, locale = 'en') {
  await p.addInitScript(
    (locale) => localStorage.setItem('neondojo.locale', locale),
    locale,
  );
  await p.goto('/');
  const white = `Estimate ${crypto.randomUUID()}`;
  const response = p.waitForResponse(
    (r) =>
      new URL(r.url()).pathname === '/api/imports' &&
      r.request().method() === 'POST',
  );
  await p
    .getByLabel(locale === 'de' ? 'Matchdatei' : 'Match file', { exact: true })
    .setInputFiles({
      name: `estimate-${crypto.randomUUID()}.sgf`,
      mimeType: 'application/octet-stream',
      buffer: Buffer.from(
        `(;GM[6]PW[${white}]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe];B[42qusu])(;GM[6]PW[${white}]PB[Opponent]MI[length:7][ws:1][bs:0];W[31hefe])`,
      ),
    });
  const match = (await (await response).json()).match;
  if (match?.id) importedMatchIds.get(p)!.add(match.id);
  await expect(p.locator('.analysis-workspace .board')).toBeVisible();
  return match;
}
test('starts GNUbg 5-ply for the selected game with an Open Sage profile', async ({
  page,
}) => {
  // A real GNUbg 5-ply (its own 4-ply) game analysis; other specs allow the same three minutes.
  test.setTimeout(240_000);
  await page.setViewportSize({ width: 1800, height: 1169 });
  const config = structuredClone(defaultConfiguration);
  config.analysis.engine = 'bgsage';
  const profile = await profileBackend(page, { configuration: config });
  await page.route('**/api/engines', (route) =>
    route.fulfill({
      json: [
        {
          id: 'gnubg',
          name: 'GNUbg',
          available: true,
          version: null,
          plies: [1, 2, 3, 4, 5],
        },
        {
          id: 'bgsage',
          name: 'Open Sage',
          available: true,
          version: null,
          plies: [2, 3],
        },
      ],
    }),
  );
  const match = await openMatch(page);
  await openMatchAnalysisOptions(page);
  const engine = page.getByLabel('Analysis engine', { exact: true });
  const depth = page.getByLabel('Match analysis depth', { exact: true });
  await expect(engine).toHaveValue('bgsage');
  await expect(depth.locator('option[value="4"]')).toBeDisabled();
  await expect(depth.locator('option[value="1"]')).toBeDisabled();
  await page.getByLabel('Select game', { exact: true }).selectOption('2');
  await engine.selectOption('gnubg');
  const estimateResponse = page.waitForResponse((response) => {
    const url = new URL(response.url());
    return (
      url.pathname === `/api/matches/${match.id}/analysis/estimate` &&
      url.searchParams.get('game_no') === '2' &&
      url.searchParams.get('engine_id') === 'gnubg' &&
      url.searchParams.get('plies') === '5'
    );
  });
  await depth.selectOption('5');
  expect(await (await estimateResponse).json()).toMatchObject({
    game_no: 2,
    engine_id: 'gnubg',
    plies: 5,
  });
  await engine.selectOption('bgsage');
  await expect(depth).toHaveValue('2');
  await engine.selectOption('gnubg');
  await depth.selectOption('5');
  await page.screenshot({ path: '/tmp/neondojo-analysis-engine-desktop.png' });
  const accepted = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname ===
        `/api/matches/${match.id}/analysis` &&
      response.request().method() === 'POST',
  );
  await page.getByRole('button', { name: 'Analyze game', exact: true }).click();
  const response = await accepted;
  expect(response.status()).toBe(202);
  const job = await response.json();
  expect(job).toMatchObject({
    game_no: 2,
    plies: 5,
    engine_snapshot: { engine_id: 'gnubg' },
  });
  expect(profile.calls.filter(({ method }) => method !== 'GET')).toEqual([]);
  expect(profile.state.current_configuration.analysis.engine).toBe('bgsage');
  await expect
    .poll(
      async () => {
        const response = await page.request.get(`/api/jobs/${job.id}`);
        return (await response.json()).status;
      },
      { timeout: 180_000 },
    )
    .toBe('completed');
  await expect(page.locator('.match-analysis-progress:visible')).toContainText(
    'Game analysis completed',
  );
  await page.getByRole('tab', { name: 'Notes', exact: true }).click();
  await expect(page.locator('.match-analysis-progress:visible')).toContainText(
    'Game analysis completed',
  );
  await openMatchAnalysisOptions(page);
  await engine.selectOption('bgsage');
  await expect(page.locator('.match-analysis-progress:visible')).toContainText(
    'Game analysis completed',
  );
  await engine.selectOption('gnubg');
  await expect(page.locator('.match-analysis-progress:visible')).toContainText(
    'Game analysis completed',
  );
});

test('shows benchmark basis before any job and follows game, scope and depth', async ({
  page,
}) => {
  const database = await page.request.post('/api/databases', {
    data: { name: `Estimate collection ${crypto.randomUUID()}` },
  });
  expect(database.status()).toBe(201);
  const requests: string[] = [],
    starts: string[] = [];
  page.on('request', (r) => {
    if (r.method() === 'POST' && /\/analysis$/.test(new URL(r.url()).pathname))
      starts.push(r.url());
  });
  await page.route('**/api/matches/*/analysis/estimate?*', (r) => {
    requests.push(r.request().url());
    return r.fulfill({ json: result(r.request().url()) });
  });
  await openMatch(page);
  await openMatchAnalysisOptions(page);
  await expect(estimate(page)).toContainText(
    'Estimated analysis time: approx. 40s',
  );
  await expect(estimate(page)).toContainText('1,000 checker moves · 3 Ply');
  await expect(estimate(page)).toContainText('GNU Backgammon 1.08.003');
  await expect(estimate(page)).toContainText('Nov 10, 2026');
  await expect(estimate(page)).toContainText('Queueing excluded');
  expect(new URL(requests.at(-1)!).searchParams.get('game_no')).toBe('1');
  expect(new URL(requests.at(-1)!).searchParams.get('engine_id')).toBe('gnubg');
  await page.getByLabel('Select game', { exact: true }).selectOption('2');
  await expect
    .poll(() => new URL(requests.at(-1)!).searchParams.get('game_no'))
    .toBe('2');
  expect(new URL(requests.at(-1)!).searchParams.get('game_no')).toBe('2');
  const controls = page.locator(
    '.workspace-analysis-command .match-analysis-controls',
  );
  for (const label of await controls.locator('label').all())
    expect((await label.boundingBox())!.width).toBeGreaterThan(100);
  await controls.getByRole('button', { name: 'Analysis options' }).click();
  await expect(estimate(page)).toBeHidden();
  const dialog = await openFullMatchAnalysis(page);
  await expect
    .poll(() => new URL(requests.at(-1)!).searchParams.has('game_no'))
    .toBe(false);
  await dialog
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('5');
  await expect(estimate(page)).toContainText(
    'Estimated analysis time: approx. 40s',
  );
  await expect(estimate(page)).toContainText('5 Ply');
  expect(starts).toEqual([]);
  await page.screenshot({
    path: '/tmp/neondojo-analysis-estimate-desktop.png',
    fullPage: true,
  });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(estimate(page)).toBeVisible();
  await expect(dialog).toBeInViewport({ ratio: 1 });
  await expect
    .poll(() => page.evaluate(() => document.documentElement.scrollWidth))
    .toBe(390);
  await page.screenshot({
    path: '/tmp/neondojo-analysis-estimate-mobile.png',
    fullPage: true,
  });
});
test('invalidates immediately and ignores late depth responses', async ({
  page,
}) => {
  let release!: () => void;
  const gate = new Promise<void>((r) => (release = r));
  let finished!: () => void;
  const done = new Promise<void>((r) => {
    finished = r;
  });
  let held = false;
  await page.route('**/api/matches/*/analysis/estimate?*', async (r) => {
    if (new URL(r.request().url()).searchParams.get('plies') === '4') {
      held = true;
      await gate;
      await r
        .fulfill({ json: result(r.request().url(), 180_000) })
        .catch(() => {});
      finished();
      return;
    }
    return r.fulfill({ json: result(r.request().url()) });
  });
  await openMatch(page);
  await openMatchAnalysisOptions(page);
  await expect(estimate(page)).toContainText('approx. 40s');
  await page
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('4');
  await expect(estimate(page)).toContainText('Estimating duration');
  await expect(estimate(page)).not.toContainText('40s');
  await expect.poll(() => held).toBe(true);
  await page
    .getByLabel('Match analysis depth', { exact: true })
    .selectOption('5');
  await expect(estimate(page)).toContainText('5 Ply');
  release();
  await done;
  await page.evaluate(
    () =>
      new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      ),
  );
  await expect(estimate(page)).toContainText('5 Ply');
  await expect(estimate(page)).not.toContainText('3m');
});
for (const reason of [
  'no_matching_benchmark',
  'engine_unavailable',
  'no_checker_moves',
  'network',
] as const) {
  test(`unavailable ${reason} is localized without blocking start`, async ({
    page,
  }) => {
    await page.route('**/api/matches/*/analysis/estimate?*', (r) =>
      reason === 'network'
        ? r.fulfill({ status: 503, json: { error: 'offline' } })
        : r.fulfill({
            json: {
              ...result(r.request().url()),
              estimated_ms: null,
              position_count: reason === 'no_checker_moves' ? 0 : 1000,
              unavailable_reason: reason,
              benchmark: null,
            },
          }),
    );
    await openMatch(page, 'de');
    await openMatchAnalysisOptions(page);
    await expect(estimate(page)).toContainText('Schätzung nicht verfügbar');
    const explanation = {
      no_matching_benchmark: 'Kein abgeschlossener Benchmark passt',
      engine_unavailable:
        'Die konfigurierte Analyse-Engine ist nicht verfügbar',
      no_checker_moves: 'Keine Checkerzüge in diesem Umfang',
      network: 'Die Schätzung konnte nicht geladen werden',
    }[reason];
    await expect(estimate(page)).toContainText(explanation);
    await expect(estimate(page)).not.toContainText('0s');
    await expect(
      page.getByRole('button', { name: 'Spiel analysieren', exact: true }),
    ).toBeEnabled();
  });
}
test('retains the estimate when confirmed settings change only appearance', async ({
  page,
}) => {
  const config = structuredClone(defaultConfiguration);
  config.general.locale = 'en';
  const backend = await profileBackend(page, { configuration: config });
  let calls = 0;
  await page.route('**/api/matches/*/analysis/estimate?*', (r) => {
    calls++;
    return r.fulfill({
      json: result(r.request().url(), calls === 1 ? 40_000 : 80_000),
    });
  });
  await openMatch(page);
  await openMatchAnalysisOptions(page);
  await expect(estimate(page)).toContainText('approx. 40s');
  backend.updateRemote({
    ...config,
    appearance: { ...config.appearance, mode: 'dark' },
  });
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.locator('html')).toHaveAttribute('data-ui-mode', 'dark');
  await expect(estimate(page)).toContainText('approx. 40s');
  expect(calls).toBe(1);
  expect(backend.calls.filter((c) => c.method !== 'GET')).toEqual([]);
});

// Mount the real component and configuration store in the browser to exercise
// the 300ms unsaved window, including when status still reads "ready".
async function mountEstimateHarness(page: Page) {
  await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
  await page.route('**/api/matches/fixture/analysis?*', (route) =>
    route.fulfill({ json: null }),
  );
  await page.goto('/');
  await page.evaluate(async () => {
    const { React: react, createRoot } = await import('/e2e/react-fixture.ts');
    const { ConfigurationStore } = await import('/src/configuration-store.ts');
    const { ConfigurationProvider, useConfirmedConfiguration } =
      await import('/src/ConfigurationProvider.tsx');
    const { defaultConfiguration } =
      await import('/src/configuration-model.ts');
    const { default: MatchPanel } = await import('/src/MatchAnalysisPanel.tsx');
    const { catalogFixture } =
      await import('/src/analysis-catalog-test-fixtures.ts');
    const config = structuredClone(defaultConfiguration);
    const profile = {
      id: 'fixture',
      name: 'Fixture',
      revision: 1,
      configuration: config,
    };
    let state = {
      revision: 1,
      current_configuration: config,
      active_profile: profile,
      differences: [],
      dirty: false,
    };
    let release = () => {};
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const store = new ConfigurationStore(
      {
        read: async () => state,
        listProfiles: async () => [profile],
        write: async (_revision: number, configuration: typeof config) => {
          await gate;
          state = {
            ...state,
            revision: 2,
            current_configuration: configuration,
          };
          return state;
        },
      },
      { getItem: () => null, setItem: () => {}, removeItem: () => {} },
    );
    const root = document.createElement('div');
    document.body.append(root);
    function ConfirmedMatch(props: Parameters<typeof MatchPanel>[0]) {
      const confirmed = useConfirmedConfiguration();
      return react.createElement(MatchPanel, {
        ...props,
        defaultPlies: confirmed?.analysis.plies ?? props.defaultPlies,
      });
    }
    function Harness() {
      const [installationKey, setInstallation] = react.useState('/old/gnubg');
      const [positionCount, setPositionCount] = react.useState(1000);
      return react.createElement(
        ConfigurationProvider,
        { store },
        react.createElement(ConfirmedMatch, {
          match: {
            id: 'fixture',
            white: 'Fixture',
            black: 'Opponent',
            position_count: positionCount,
          },
          catalog: catalogFixture(),
          gameNo: 1,
          defaultPlies: 3,
          configured: true,
          analysisInstallationKey: installationKey,
        }),
        react.createElement(
          'button',
          {
            onClick: () =>
              store.updateCurrent((c: typeof config) => ({
                ...c,
                analysis: { ...c.analysis, plies: 4 },
              })),
          },
          'Unsaved engine edit',
        ),
        react.createElement(
          'button',
          { onClick: release },
          'Confirm engine edit',
        ),
        react.createElement(
          'button',
          { onClick: () => setInstallation('/new/gnubg') },
          'Replace engine installation',
        ),
        react.createElement(
          'button',
          { onClick: () => setPositionCount(1002) },
          'Append checker moves',
        ),
      );
    }
    createRoot(root).render(react.createElement(Harness));
  });
}

test('estimates use confirmed settings while an edit is still pending', async ({
  page,
}) => {
  let calls = 0;
  await page.route('**/api/matches/fixture/analysis/estimate?*', (route) => {
    calls++;
    return route.fulfill({
      json: result(route.request().url(), calls === 1 ? 40_000 : 80_000),
    });
  });
  await mountEstimateHarness(page);
  await expect(estimate(page)).toContainText('approx. 40s');
  await page
    .getByRole('button', { name: 'Unsaved engine edit', exact: true })
    .click();
  await expect(estimate(page)).toContainText('approx. 40s');
  expect(calls).toBe(1);
  await page
    .getByRole('button', { name: 'Confirm engine edit', exact: true })
    .click();
  await expect(estimate(page)).toContainText('approx. 1m 20s');
  expect(calls).toBe(2);
});

test('invalidates when installation changes while depth stays the same', async ({
  page,
}) => {
  let calls = 0;
  await page.route('**/api/matches/fixture/analysis/estimate?*', (route) => {
    calls++;
    return route.fulfill({
      json: result(route.request().url(), calls === 1 ? 40_000 : 120_000),
    });
  });
  await mountEstimateHarness(page);
  await expect(estimate(page)).toContainText('approx. 40s');
  await page
    .getByRole('button', { name: 'Replace engine installation', exact: true })
    .click();
  await expect(estimate(page)).toContainText('approx. 2m');
  expect(calls).toBe(2);
});

test('hides pre-start estimate during submission and queued engine work', async ({
  page,
}) => {
  await page.route('**/api/matches/*/analysis/estimate?*', (route) =>
    route.fulfill({ json: result(route.request().url()) }),
  );
  const match = await openMatch(page);
  await openMatchAnalysisOptions(page);
  await expect(estimate(page)).toContainText('approx. 40s');
  let release!: () => void;
  const gate = new Promise<void>((r) => {
    release = r;
  });
  let accepted = false;
  const job = {
    id: 'fixture-queued',
    match_uuid: match.id,
    game_no: 1,
    kind: 'match',
    status: 'queued',
    plies: 3,
    progress: 0,
    attempts: 0,
    completed_games: 0,
    total_games: 1,
    result: null,
    error: null,
    created_at: '2026-11-10T12:00:00Z',
    updated_at: '2026-11-10T12:00:00Z',
  };
  await page.route('**/api/matches/*/analysis', async (route) => {
    await gate;
    accepted = true;
    return route.fulfill({ json: job });
  });
  await page.route('**/api/matches/*/analysis?*', (route) =>
    route.fulfill({
      json:
        accepted &&
        new URL(route.request().url()).searchParams.get('engine_id') === 'gnubg'
          ? job
          : null,
    }),
  );
  await page.route('**/api/jobs/fixture-queued', (route) =>
    route.fulfill({ json: job }),
  );
  await page.route('**/api/jobs/fixture-queued/statistics', (route) =>
    route.fulfill({ json: { job_id: job.id, games: [], summary: null } }),
  );
  await page.route('**/api/matches/*/review-decisions?*', (route) =>
    route.fulfill({ json: { items: [], next_cursor: null } }),
  );
  await page.route('**/api/jobs/fixture-queued/events', (route) =>
    route.fulfill({ contentType: 'text/event-stream', body: '' }),
  );
  await page.getByRole('button', { name: 'Analyze game', exact: true }).click();
  await expect(estimate(page)).toHaveCount(0);
  release();
  const background = page.locator(
    '.analysis-commandbar .analysis-running-jobs',
  );
  await expect(background).toContainText('Analysis queued');
  await openMatchAnalysisOptions(page);
  const engine = page.getByLabel('Analysis engine', { exact: true });
  await engine.selectOption('bgsage');
  await expect(page.locator('.match-analysis-progress:visible')).toHaveCount(0);
  await expect(page.locator('.analysis-running-jobs')).toContainText('GNUbg');
  await engine.selectOption('gnubg');
  await expect(background).toContainText('Analysis queued');
  await expect(estimate(page)).toHaveCount(0);
});

test('refreshes when mutable match gains moves at the same match, game and depth', async ({
  page,
}) => {
  let calls = 0;
  await page.route('**/api/matches/fixture/analysis/estimate?*', (route) => {
    calls++;
    return route.fulfill({
      json: {
        ...result(route.request().url(), calls === 1 ? 40_000 : 80_000),
        position_count: calls === 1 ? 1000 : 1002,
      },
    });
  });
  await mountEstimateHarness(page);
  await expect(estimate(page)).toContainText('1,000 checker moves');
  await page
    .getByRole('button', { name: 'Append checker moves', exact: true })
    .click();
  await expect(estimate(page)).toContainText('1,002 checker moves');
  await expect(estimate(page)).toContainText('approx. 1m 20s');
  expect(calls).toBe(2);
});

for (const malformed of ['date', 'reason'] as const) {
  test(`malformed optional estimate ${malformed} remains non-blocking`, async ({
    page,
  }) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.route('**/api/matches/*/analysis/estimate?*', (route) => {
      const data = result(route.request().url());
      const invalid =
        malformed === 'date'
          ? {
              ...data,
              benchmark: { ...data.benchmark, finished_at: 'not-a-date' },
            }
          : {
              ...data,
              estimated_ms: null,
              benchmark: null,
              unavailable_reason: 'unexpected_server_reason',
            };
      return route.fulfill({ json: invalid });
    });
    await openMatch(page);
    await openMatchAnalysisOptions(page);
    await expect(estimate(page)).toContainText('Estimate unavailable');
    await expect(
      page.getByRole('button', { name: 'Analyze game', exact: true }),
    ).toBeEnabled();
    expect(errors).toEqual([]);
  });
}
