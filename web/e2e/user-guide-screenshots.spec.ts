// Recaptures the screenshots in docs/user-guide/images from Playwright's
// disposable database. It seeds the database only with the public fixtures in
// backend/tests/fixtures, runs real GNUbg and NeonGammon work and writes WebP
// files with `cwebp`. Opt in with NEONDOJO_GUIDE_SCREENSHOTS=1 and set
// NEONDOJO_SAGE_PYTHON to the NeonGammon interpreter. NEONDOJO_GUIDE_INTERFACE_THEME,
// NEONDOJO_GUIDE_DISPLAY_MODE and NEONDOJO_GUIDE_BOARD_THEME choose the appearance.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  test,
  expect,
  type APIRequestContext,
  type Locator,
  type Page,
} from '@playwright/test';
import type { components } from '../src/generated-api';

type Schemas = components['schemas'];

const root = fileURLToPath(new URL('../../', import.meta.url));
const fixtures = join(root, 'backend/tests/fixtures');
const output =
  process.env.NEONDOJO_GUIDE_SCREENSHOTS_DIR ??
  join(root, 'docs/user-guide/images');
const sagePython = process.env.NEONDOJO_SAGE_PYTHON;

// Optional appearance for the captures. Unset values keep the default
// configuration; the backend validates the values that are set.
type Appearance = Schemas['ConfigurationAppearance'];
const interfaceTheme = process.env.NEONDOJO_GUIDE_INTERFACE_THEME as Appearance['ui_theme'] | undefined;
const displayMode = process.env.NEONDOJO_GUIDE_DISPLAY_MODE as Appearance['mode'] | undefined;
const boardTheme = process.env.NEONDOJO_GUIDE_BOARD_THEME;

/** Accepts a configured board theme ID or a BGBlitz archive name without the `bgblitz:` prefix. */
function boardThemeId(value: string): string {
  const options = JSON.parse(readFileSync(join(root, 'shared/configuration-options.json'), 'utf8')) as {
    board_themes: string[];
  };
  const id = [value, `bgblitz:${value}`].find((candidate) => options.board_themes.includes(candidate));
  if (!id) {
    throw new Error(
      `Unknown NEONDOJO_GUIDE_BOARD_THEME "${value}". Available: ${options.board_themes.join(', ')}`,
    );
  }
  return id;
}

test.skip(
  process.env.NEONDOJO_GUIDE_SCREENSHOTS !== '1' || !sagePython,
  'Set NEONDOJO_GUIDE_SCREENSHOTS=1 and NEONDOJO_SAGE_PYTHON to recapture the user guide',
);

test.use({
  viewport: { width: 1800, height: 1169 },
  deviceScaleFactor: 1,
  // "system" follows the browser, which stays light unless dark is requested.
  colorScheme: displayMode === 'dark' ? 'dark' : 'light',
  locale: 'en-US',
  timezoneId: 'Europe/Berlin',
});

type Clip = { x: number; y: number; width: number; height: number };

async function json<T>(response: {
  ok(): boolean;
  status(): number;
  text(): Promise<string>;
}): Promise<T> {
  const text = await response.text();
  expect(response.ok(), `${response.status()} ${text}`).toBe(true);
  return (text ? JSON.parse(text) : null) as T;
}

async function until<T>(
  read: () => Promise<T>,
  done: (value: T) => boolean,
  label: string,
  timeout = 900_000,
) {
  const end = Date.now() + timeout;
  for (;;) {
    const value = await read();
    if (done(value)) return value;
    if (Date.now() > end) throw new Error(`Timed out waiting for ${label}`);
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }
}

async function importFixture(
  request: APIRequestContext,
  file: string,
  databaseId?: string,
) {
  const query = databaseId ? `?database_id=${databaseId}` : '';
  const result = await json<Schemas['ImportResponse']>(
    await request.post(`/api/imports${query}`, {
      multipart: {
        file: {
          name: file,
          mimeType: 'application/octet-stream',
          buffer: readFileSync(join(fixtures, file)),
        },
      },
    }),
  );
  return result.match.id;
}

type AnalysisJob = { id: string; status: string; engine_snapshot: { engine_id: string } };

async function analyze(
  request: APIRequestContext,
  matchId: string,
  body: { plies: number; engine_id: string; game_no?: number },
) {
  return json<AnalysisJob>(
    await request.post(`/api/matches/${matchId}/analysis`, { data: body }),
  );
}

async function waitForAnalyses(request: APIRequestContext, matchIds: string[]) {
  for (const id of matchIds)
    await until(
      async () =>
        json<AnalysisJob[]>(await request.get(`/api/matches/${id}/analysis/jobs`)),
      (jobs) => jobs.length > 0 && jobs.every((job) => job.status === 'completed'),
      `analysis of ${id}`,
    );
}

// Plays a short bot match through the play API with mostly first-choice
// moves, confirms feedback pauses and saves the learning review.
async function playBotMatch(
  request: APIRequestContext,
  matchLength: number,
  mode: Schemas['PlayMode'],
  seed: number,
) {
  let state = seed;
  const random = () => {
    state = (state * 1103515245 + 12345) % 2147483648;
    return state / 2147483648;
  };
  let session = await json<Schemas['PlaySession']>(
    await request.post('/api/play/sessions', {
      data: { human_player: 0, match_length: matchLength, play_mode: mode },
    }),
  );
  for (let step = 0; step < 3_000 && session.phase !== 'match_over'; step++) {
    const pending = session.pending_decision;
    let body: Record<string, unknown>;
    if (pending?.action) {
      body = { version: session.version, action: pending.action, decision_id: pending.id };
      if (pending.move_index !== null && pending.move_index !== undefined)
        body.move = pending.move_index;
    } else {
      const actions: Record<string, string> = {
        opening_roll: 'opening_roll',
        opening_result: 'start_opening_move',
        roll: 'roll',
        game_over: 'next_game',
      };
      body = { version: session.version, action: actions[session.phase] };
      if (session.phase === 'move') {
        const moves = session.legal_moves;
        body.action = moves.length ? 'move' : 'pass';
        if (moves.length)
          body.move = random() < 0.8 ? 0 : Math.floor(random() * moves.length);
      } else if (session.phase === 'double_response') {
        body.action = random() < 0.7 ? 'take' : 'drop';
      }
    }
    session = await json<Schemas['PlaySession']>(
      await request.post(`/api/play/sessions/${session.id}/actions`, { data: body }),
    );
  }
  expect(session.phase).toBe('match_over');
  await json(
    await request.post(`/api/play/sessions/${session.id}/review`, {
      data: { version: session.version },
    }),
  );
  return session.id;
}

async function settle(page: Page, ms = 600) {
  await page.waitForLoadState('networkidle').catch(() => {});
  await page.waitForTimeout(ms);
}

// Numbered orange markers for the legends in the user guide.
async function callouts(
  page: Page,
  items: { n: number; loc: Locator; at?: string; dx?: number; dy?: number }[],
) {
  const boxes: { n: number; x: number; y: number }[] = [];
  for (const { n, loc, at = 'tl', dx = 0, dy = 0 } of items) {
    const b = await loc.boundingBox();
    if (!b) throw new Error(`Callout ${n}: element is not visible`);
    let x = b.x;
    let y = b.y;
    if (at.includes('r')) x = b.x + b.width;
    if (at.includes('c')) x = b.x + b.width / 2;
    if (at.includes('b')) y = b.y + b.height;
    if (at.includes('m')) y = b.y + b.height / 2;
    boxes.push({ n, x: x + dx, y: y + dy });
  }
  await page.evaluate((markers) => {
    for (const { n, x, y } of markers) {
      const marker = document.createElement('div');
      marker.className = 'guide-callout';
      marker.textContent = String(n);
      Object.assign(marker.style, {
        position: 'fixed',
        left: `${x - 11}px`,
        top: `${y - 11}px`,
        width: '22px',
        height: '22px',
        borderRadius: '50%',
        background: '#e8590c',
        color: '#fff',
        font: '700 12px/22px -apple-system, system-ui, sans-serif',
        textAlign: 'center',
        zIndex: '2147483647',
        boxShadow: '0 0 0 2px #fff',
        pointerEvents: 'none',
      });
      document.body.appendChild(marker);
    }
  }, boxes);
}

async function clearCallouts(page: Page) {
  await page.evaluate(() =>
    document.querySelectorAll('.guide-callout').forEach((marker) => marker.remove()),
  );
}

async function save(
  page: Page,
  name: string,
  target: { clip?: Clip; locator?: Locator } = {},
) {
  await page.mouse.move(1799, 1168);
  await page.waitForTimeout(250);
  const png = test.info().outputPath(`${name}.png`);
  if (target.locator)
    await target.locator.screenshot({ path: png, animations: 'disabled' });
  else await page.screenshot({ path: png, clip: target.clip, animations: 'disabled' });
  execFileSync('cwebp', ['-quiet', '-q', '86', '-m', '6', png, '-o', join(output, `${name}.webp`)]);
}

// From an element's top-left corner down to its last visible content.
async function contentClip(locator: Locator, pad = 12): Promise<Clip> {
  return locator.evaluate((element, padding) => {
    const box = element.getBoundingClientRect();
    let bottom = box.top;
    for (const child of element.querySelectorAll('*')) {
      if (!(child as HTMLElement).offsetParent) continue;
      const b = child.getBoundingClientRect();
      if (b.width > 0 && b.height > 0 && b.bottom > bottom && b.bottom <= box.bottom + 1)
        bottom = b.bottom;
    }
    return {
      x: Math.max(0, box.left),
      y: Math.max(0, box.top),
      width: box.width,
      height: Math.min(box.height, bottom - box.top + padding),
    };
  }, pad);
}

// Full width from the top of the page to a workspace's last visible content.
async function panelClip(panel: Locator, pad = 16): Promise<Clip> {
  const bottom = await panel.evaluate((element) => {
    let last = 0;
    for (const child of element.querySelectorAll('*')) {
      if (!(child as HTMLElement).offsetParent) continue;
      const b = child.getBoundingClientRect();
      if (b.width > 0 && b.height > 0 && b.bottom > last) last = b.bottom;
    }
    return last;
  });
  return { x: 0, y: 0, width: 1800, height: Math.min(1169, Math.ceil(bottom + pad)) };
}

const workspace = (page: Page, id: string) => page.locator(`#workspace-panel-${id}`);

async function openWorkspace(page: Page, name: string) {
  await page.locator('header.app-header').getByRole('tab', { name, exact: true }).click();
  await settle(page);
}

async function selectByPrefix(select: Locator, prefix: string) {
  // Options arrive asynchronously after the workspace opens.
  await expect
    .poll(async () => (await select.locator('option').allTextContents()).some((option) => option.startsWith(prefix)))
    .toBe(true);
  const options = await select.locator('option').allTextContents();
  const label = options.find((option) => option.startsWith(prefix));
  if (!label) throw new Error(`No option starting with ${prefix}`);
  await select.selectOption({ label });
}

test('recapture the user guide screenshots', async ({ page }) => {
  test.setTimeout(7_200_000);
  mkdirSync(output, { recursive: true });
  execFileSync('cwebp', ['-version']);
  const request = page.request;
  let wc2014 = '';

  await test.step('seed the demo library', async () => {
    await page.addInitScript(() => localStorage.setItem('neondojo.locale', 'en'));
    await page.goto('/');
    await expect(page.getByRole('button', { name: /^Profile:/ })).toBeVisible();

    // Neutral player name and the requested appearance, saved into the profile
    // so the header is not "modified".
    const state = await json<Schemas['ConfigurationState']>(await request.get('/api/configuration'));
    const configuration = state.current_configuration;
    configuration.general.player_name = 'Sam';
    if (interfaceTheme) configuration.appearance.ui_theme = interfaceTheme;
    if (displayMode) configuration.appearance.mode = displayMode;
    if (boardTheme) configuration.appearance.board_theme = boardThemeId(boardTheme);
    const updated = await json<Schemas['ConfigurationState']>(
      await request.put('/api/configuration', {
        data: { expected_revision: state.revision, configuration },
      }),
    );
    await json(
      await request.put(`/api/profiles/${updated.active_profile.id}/snapshot`, {
        data: {
          expected_state_revision: updated.revision,
          expected_profile_revision: updated.active_profile.revision,
          configuration: updated.current_configuration,
        },
      }),
    );
    const sage = await json<{ version: number }>(await request.get('/api/engines/bgsage'));
    await json(
      await request.put('/api/engines/bgsage', {
        data: { version: sage.version, path: sagePython },
      }),
    );

    const tournaments = await json<{ id: string }>(
      await request.post('/api/databases', { data: { name: 'Tournaments' } }),
    );
    wc2014 = await importFixture(request, 'WC2014.xg', tournaments.id);
    const others = [
      await importFixture(request, 'tutorial.mat', tournaments.id),
      await importFixture(request, 'tutorial.bgf', tournaments.id),
      await importFixture(request, 'game-results-match.sgf', tournaments.id),
    ];
    await importFixture(request, 'hedgehog-cdf-analyzed.ogxm', tournaments.id);
    // A second copy of the same match gives Akiko Yazawa recurring mistakes.
    const copy = await importFixture(request, 'WC2014.mat');

    await analyze(request, wc2014, { plies: 3, engine_id: 'gnubg' });
    for (const id of [...others, copy]) await analyze(request, id, { plies: 3, engine_id: 'gnubg' });
    await analyze(request, wc2014, { plies: 2, engine_id: 'bgsage', game_no: 1 });
    await waitForAnalyses(request, [wc2014, ...others, copy]);

    const job = (
      await json<AnalysisJob[]>(await request.get(`/api/matches/${wc2014}/analysis/jobs`))
    ).find((entry) => entry.engine_snapshot.engine_id === 'gnubg')!;
    const training = await json<{ id: string }>(
      await request.post('/api/decks', { data: { name: 'Training' } }),
    );
    for (const player of [0, 1])
      await json(
        await request.post(`/api/decks/${training.id}/cards`, {
          data: { job_id: job.id, match_id: wc2014, min_loss: 0.05, player },
        }),
      );
    const errors = await json<{ id: string }>(
      await request.post('/api/decks', { data: { name: 'Yazawa errors' } }),
    );
    await json(
      await request.post(`/api/decks/${errors.id}/cards`, {
        data: { job_id: job.id, match_id: wc2014, min_loss: 0.02, player: 0, tags: ['cube'] },
      }),
    );

    const positions: { id: string }[] = [];
    let after: string | null | undefined;
    do {
      const page_ = await json<{ items: { id: string }[]; next_cursor?: string | null }>(
        await request.get(
          `/api/positions?match_id=${wc2014}&limit=100${after ? `&after=${after}` : ''}`,
        ),
      );
      positions.push(...page_.items);
      after = page_.next_cursor;
    } while (after);
    for (const position of positions.filter((_, index) => index % 41 === 3).slice(0, 10))
      await json(await request.put(`/api/positions/${position.id}/bookmark`));

    const competition = await playBotMatch(request, 3, 'competition', 7);
    await playBotMatch(request, 3, 'coach', 11);
    await playBotMatch(request, 1, 'tutor', 5);
    const sessions = await json<Schemas['PlaySession'][]>(await request.get('/api/play/sessions'));
    const libraryMatch = sessions.find((entry) => entry.id === competition)?.library_match_id;
    if (libraryMatch) {
      await analyze(request, libraryMatch, { plies: 3, engine_id: 'gnubg' });
      await waitForAnalyses(request, [libraryMatch]);
    }

    await json(
      await request.post('/api/benchmarks', { data: { engine_id: 'gnubg', plies: [1, 2, 3] } }),
    );
    await until(
      async () => json<{ active: boolean }>(await request.get('/api/benchmarks/status')),
      (status) => !status.active,
      'GNUbg benchmark',
    );
    await json(await request.post('/api/benchmarks', { data: { engine_id: 'bgsage', plies: [1] } }));
    await until(
      async () => json<{ active: boolean }>(await request.get('/api/benchmarks/status')),
      (status) => !status.active,
      'NeonGammon benchmark',
    );
    const duel = await json<{ id: string }>(
      await request.post('/api/engine-matches', {
        data: {
          a: { engine_id: 'gnubg', plies: 2 },
          b: { engine_id: 'bgsage', plies: 1 },
          count: 6,
          format: 'match',
          match_length: 3,
        },
      }),
    );
    const duelRun = await until(
      async () =>
        json<{ series: { status: string; error: string | null } }>(
          await request.get(`/api/engine-matches/${duel.id}`),
        ),
      (run) => ['completed', 'failed', 'cancelled'].includes(run.series.status),
      'engine duel',
    );
    // A failed series would end up in extras-duels.webp; stop with its cause instead.
    expect(duelRun.series.status, duelRun.series.error ?? 'engine duel did not complete').toBe('completed');

    // Practice answers for Practice history, given through the dialog.
    await page.reload();
    await openWorkspace(page, 'Study');
    const learning = workspace(page, 'learning');
    await learning.getByRole('tab', { name: 'My recurring mistakes' }).click();
    await selectByPrefix(learning.getByRole('combobox', { name: 'Player' }).first(), 'Akiko Yazawa');
    await settle(page, 1500);
    for (const [kind, choices] of [
      ['Missed double', ['No double', 'Double', 'Double']],
      ['Wrong take', ['Take double', 'Pass double']],
    ] as const) {
      await learning
        .locator('tbody tr')
        .filter({ hasText: kind })
        .first()
        .getByRole('button', { name: /Practice/ })
        .click();
      const dialog = page.getByRole('dialog').filter({ hasText: 'Practice recurring mistakes' });
      await dialog.waitFor();
      for (let index = 0; index < 8; index++) {
        await dialog.getByRole('button', { name: choices[index % choices.length], exact: true }).click({ timeout: 30_000 });
        await dialog.getByRole('button', { name: 'Check answer' }).click();
        await settle(page, 800);
        const next = dialog.getByRole('button', { name: 'Next', exact: true });
        if ((await next.count()) === 0 || !(await next.isEnabled())) break;
        await next.click();
        await settle(page, 600);
      }
      await dialog.getByRole('button', { name: 'Close', exact: true }).last().click();
      await settle(page, 600);
    }
  });

  const openMatch = async () => {
    await page.goto('/');
    await openWorkspace(page, 'Match database');
    await page.locator(`tr[data-match-id="${wc2014}"]`).dblclick();
    await workspace(page, 'analysis').locator('h1').waitFor();
    await settle(page, 1200);
  };

  await test.step('header and analysis', async () => {
    await page.goto('/');
    await openWorkspace(page, 'Analysis');
    const drop = await page.getByText('Drop a match file here').locator('xpath=..').boundingBox();
    await save(page, 'analysis-welcome', {
      clip: { x: 0, y: 0, width: 1800, height: Math.ceil(drop!.y + drop!.height + 80) },
    });

    await openMatch();
    const header = page.locator('header.app-header');
    await callouts(page, [
      ...['Playing', 'Analysis', 'Study', 'Positions', 'Match database', 'Player database', 'Extras'].map(
        (name, index) => ({
          n: index + 1,
          loc: header.getByRole('tab', { name, exact: true }),
          at: 'ct',
          dy: -3,
        }),
      ),
      { n: 8, loc: header.getByRole('button', { name: /^Profile/ }).first(), at: 'lm', dx: -14 },
      { n: 9, loc: header.getByRole('button', { name: 'Settings' }), at: 'rm', dx: 16 },
    ]);
    const headerBox = await header.boundingBox();
    await save(page, 'header', {
      clip: { x: 0, y: 0, width: 1800, height: Math.ceil(headerBox!.height) + 1 },
    });
    await clearCallouts(page);

    const analysis = workspace(page, 'analysis');
    await analysis.getByRole('button', { name: /^1\.6\s*24\/18/ }).first().click();
    await settle(page, 800);
    await callouts(page, [
      { n: 1, loc: analysis.locator('h1'), dx: -4, dy: -4 },
      { n: 2, loc: analysis.getByLabel('Displayed analysis'), dx: -4, dy: -4 },
      { n: 3, loc: analysis.getByLabel('Select game'), dx: -4, dy: -4 },
      { n: 4, loc: analysis.getByRole('tab', { name: 'Checker', exact: true }), dx: -4, dy: -4 },
      { n: 5, loc: analysis.getByRole('button', { name: 'Analyze game' }), dx: -4, dy: -4 },
      { n: 6, loc: analysis.locator('h3', { hasText: 'Move history' }), dx: -10, dy: -6 },
      { n: 7, loc: analysis.getByRole('button', { name: 'Initial', exact: true }), dx: -6, dy: -6 },
      { n: 8, loc: analysis.getByRole('button', { name: 'Save position' }), at: 'lm', dx: -16 },
      { n: 9, loc: analysis.getByRole('tab', { name: 'Candidate moves' }), dx: -6, dy: -6 },
      {
        n: 10,
        loc: analysis.locator('.position-code').first().getByText(/position id/i).first(),
        at: 'lm',
        dx: -16,
      },
    ]);
    await save(page, 'analysis-checker');
    await clearCallouts(page);

    const evaluation = analysis.locator('.overview-evaluation');
    const results = (name: string) => page.getByRole('tab', { name, exact: true });
    await save(page, 'analysis-candidates', { clip: await contentClip(evaluation) });
    await results('Compare moves').click();
    await settle(page);
    await save(page, 'analysis-compare', { clip: await contentClip(evaluation) });
    await results('Dice equity').click();
    await analysis.getByRole('button', { name: 'Delete dice equity' }).waitFor({ timeout: 300_000 });
    await settle(page, 800);
    await save(page, 'analysis-dice', { clip: await contentClip(evaluation) });
    await results('Game rolls').click();
    await settle(page);
    await save(page, 'analysis-rolls', { clip: await contentClip(evaluation) });
    await results('Candidate moves').click();

    await analysis.getByRole('button', { name: 'Analysis options' }).click();
    await settle(page);
    const popover = (await analysis.locator('.match-analysis-popover').boundingBox())!;
    const button = (await analysis.getByRole('button', { name: 'Analyze game' }).boundingBox())!;
    const left = Math.min(popover.x, button.x) - 14;
    const top = button.y - 14;
    const right = Math.min(1800, Math.max(popover.x + popover.width, button.x + button.width) + 14);
    await save(page, 'analysis-options', {
      clip: { x: left, y: top, width: right - left, height: popover.y + popover.height + 14 - top },
    });
    await page.keyboard.press('Escape');

    for (const [name, file] of [
      ['Statistics', 'analysis-statistics'],
      ['Notes', 'analysis-notes'],
    ]) {
      await analysis.getByRole('tab', { name, exact: true }).click();
      await settle(page, 900);
      await save(page, file);
    }
    await analysis.getByRole('tab', { name: 'Study cards', exact: true }).click();
    await settle(page, 900);
    await selectByPrefix(analysis.getByLabel('Choose deck'), 'Training');
    await settle(page);
    await save(page, 'analysis-study-cards');
    await analysis.getByRole('tab', { name: 'Cube', exact: true }).click();
    await settle(page, 900);
    await analysis.getByRole('button', { name: 'Next error' }).first().click();
    await settle(page, 1500);
    await save(page, 'analysis-cube');
  });

  await test.step('match database, players and positions', async () => {
    await page.goto('/');
    await openWorkspace(page, 'Match database');
    const database = workspace(page, 'database');
    await database.getByRole('button', { name: /^Tournaments/ }).click();
    await settle(page, 900);
    const checkbox = database.locator('tbody tr[data-match-id] input[type=checkbox]').last();
    await checkbox.check();
    await settle(page, 500);
    await callouts(page, [
      { n: 1, loc: database.getByRole('button', { name: 'Import match' }), dx: -4, dy: -4 },
      { n: 2, loc: database.locator('h2,h3', { hasText: 'Databases' }).first(), dx: -12, dy: -8 },
      { n: 3, loc: database.getByLabel('Search matches'), dx: -4, dy: -4 },
      { n: 4, loc: database.getByRole('button', { name: 'Filters', exact: true }), dx: -4, dy: -4 },
      { n: 5, loc: database.getByText(/\d+ match(es)? selected/).first(), at: 'lm', dx: -18 },
      { n: 6, loc: database.locator('thead').first(), dx: -4, dy: -8 },
    ]);
    const table = (await database.locator('table').first().boundingBox())!;
    const sidebar = (await database
      .locator('h2,h3', { hasText: 'Databases' })
      .first()
      .locator('xpath=ancestor::*[self::aside or self::nav or self::section][1]')
      .boundingBox())!;
    const bottom = Math.max(table.y + table.height, sidebar.y + sidebar.height) + 48;
    await save(page, 'database-list', {
      clip: { x: 0, y: 0, width: 1800, height: Math.min(1169, Math.ceil(bottom)) },
    });
    await clearCallouts(page);
    await checkbox.uncheck();
    await database.getByRole('button', { name: /^All databases/ }).click();
    await database.getByRole('button', { name: 'Filters', exact: true }).click();
    await settle(page, 800);
    await save(page, 'database-filters');

    await openWorkspace(page, 'Player database');
    const players = workspace(page, 'players');
    await players.getByRole('button', { name: 'Akiko Yazawa', exact: true }).first().click();
    await settle(page, 1200);
    await save(page, 'players', { clip: await panelClip(players) });

    await openWorkspace(page, 'Positions');
    const positions = workspace(page, 'positions');
    await positions.locator('tbody tr[data-position-id]').nth(1).click();
    await settle(page, 1000);
    await save(page, 'positions-saved');
    await positions.getByRole('button', { name: 'All analyzed decisions' }).click();
    await settle(page, 800);
    await positions
      .getByRole('combobox', { name: 'Player' })
      .first()
      .selectOption({ label: 'Akiko Yazawa' });
    const databases = positions.getByRole('combobox', { name: 'Database' });
    for (let index = 0; index < (await databases.count()); index++)
      if (await databases.nth(index).isVisible())
        await databases.nth(index).selectOption({ label: 'Tournaments' });
    await positions.getByRole('button', { name: /Search decisions/ }).click();
    await settle(page, 1500);
    await positions.locator('tbody tr').first().click();
    await settle(page, 1200);
    await save(page, 'positions-decisions');
  });

  await test.step('study', async () => {
    await page.goto('/');
    await openWorkspace(page, 'Study');
    const learning = workspace(page, 'learning');
    const player = learning.getByRole('combobox', { name: 'Player' }).first();
    if (await player.isVisible()) await selectByPrefix(player, 'Akiko Yazawa');
    await settle(page, 1500);
    await save(page, 'study-overview', { clip: await panelClip(learning) });

    await learning.getByRole('tab', { name: 'Flashcards & Anki' }).click();
    await settle(page, 1000);
    await selectByPrefix(learning.getByRole('combobox', { name: 'Choose deck' }).first(), 'Training');
    await settle(page, 1200);
    await learning.locator('tbody tr').first().click();
    await settle(page, 1000);
    await save(page, 'study-flashcards');

    await learning.getByRole('tab', { name: 'Reports' }).click();
    await settle(page, 1200);
    await save(page, 'study-reports', { clip: await panelClip(learning) });

    await learning.getByRole('tab', { name: 'My recurring mistakes' }).click();
    await settle(page, 1000);
    await selectByPrefix(learning.getByRole('combobox', { name: 'Player' }).first(), 'Akiko Yazawa');
    await settle(page, 1800);
    await save(page, 'study-mistakes', { clip: await panelClip(learning) });

    await learning.getByRole('tab', { name: 'Practice history' }).click();
    await settle(page, 1200);
    await save(page, 'study-history', { clip: await panelClip(learning) });

    await learning.getByRole('tab', { name: 'Overview' }).click();
    await learning.getByRole('button', { name: 'Study due cards' }).click();
    await settle(page, 2000);
    await save(page, 'study-card');
  });

  await test.step('extras', async () => {
    await page.goto('/');
    await openWorkspace(page, 'Extras');
    const extras = workspace(page, 'extras');
    for (const [name, file, wait] of [
      ['Benchmark', 'extras-benchmark', 1200],
      ['Engine duels', 'extras-duels', 1500],
      ['Database backup', 'extras-backup', 1000],
      ['Versions', 'extras-versions', 2500],
    ] as const) {
      await extras.getByRole('tab', { name, exact: true }).click();
      await settle(page, wait);
      await save(page, file, { clip: await panelClip(extras) });
    }
  });

  await test.step('settings', async () => {
    await page.goto('/');
    await page.locator('header.app-header').getByRole('button', { name: 'Settings' }).click();
    const dialog = page.getByRole('dialog').filter({ hasText: 'Personal preferences' }).first();
    await dialog.waitFor();
    for (const category of ['General', 'Appearance', 'Analysis', 'Playing', 'Board', 'Profiles']) {
      await dialog
        .getByRole('tab', { name: category, exact: true })
        .or(dialog.getByRole('button', { name: category, exact: true }))
        .first()
        .click();
      await settle(page, 1200);
      await save(page, `settings-${category.toLowerCase()}`, { locator: dialog });
    }
  });

  await test.step('playing', async () => {
    await page.goto('/');
    await openWorkspace(page, 'Playing');
    const setup = page.getByRole('dialog').filter({ hasText: 'New match' }).first();
    await setup.waitFor();
    await settle(page, 800);
    await save(page, 'playing-setup', { locator: setup });
    await setup.getByRole('button', { name: 'Start match' }).click();
    const play = workspace(page, 'play');
    await play.getByRole('button', { name: 'Confirm turn' }).waitFor({ timeout: 60_000 });
    await settle(page, 1500);
    const clip = await play.evaluate((element) => {
      let x0 = 1e9;
      let y0 = 1e9;
      let x1 = 0;
      let y1 = 0;
      for (const child of element.querySelectorAll('*')) {
        if (!(child as HTMLElement).offsetParent || child.closest('footer')) continue;
        const b = child.getBoundingClientRect();
        if (b.width < 2 || b.height < 2 || b.width > 1700) continue;
        x0 = Math.min(x0, b.left);
        y0 = Math.min(y0, b.top);
        x1 = Math.max(x1, b.right);
        y1 = Math.max(y1, b.bottom);
      }
      const x = Math.max(0, x0 - 16);
      const y = Math.max(0, y0 - 16);
      return { x, y, width: Math.min(1800, x1 + 16) - x, height: y1 - y0 + 32 };
    });
    await save(page, 'playing-match', { clip });
  });
});
