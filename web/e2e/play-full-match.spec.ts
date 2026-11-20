import { rollDice, doubleCube, passDice } from './play-board-actions';
import { finishNextGame, watchOpening } from './play-opening-helpers';
import { test, expect, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, chmodSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { PlaySession } from '../src/play-api';
import type { LibraryMatch } from '../src/library-api';
const archivedMatches = new Map<string, string>();
async function archivedMatch(page: Page, session: PlaySession) {
  const response = await page.request.get(
    `/api/library/matches?database_id=00000000-0000-0000-0000-000000000003&q=${session.id}`,
  );
  expect(response.status()).toBe(200);
  const rows = (await response.json()).items as LibraryMatch[];
  expect(rows).toHaveLength(1);
  const entry = rows[0];
  expect(entry.play_session_id).toBe(session.id);
  expect(entry.play_phase).toBe(session.phase);
  expect(entry.white_score).toBe(session.scores[0]);
  expect(entry.black_score).toBe(session.scores[1]);
  expect(entry.source_available).toBe(true);
  return entry;
}
import {
  orderedLegalTurns,
  relativePoint,
  type CheckerStep,
} from '../src/play-state';

/** GNUbg writes its own level (one below NeonDojo's 1-ply = raw net) into the SGF. */
const gnubgLevel = (plies: number) => plies - 1;

test.skip(
  !process.env.NEONDOJO_FULL_MATCH_TEST,
  'Requires a disposable real GNUbg play service',
);
test.describe.configure({ mode: 'serial' });
// Keep long native matches fast while still checking the supported fade path.
test.use({ reducedMotion: 'reduce' });
test.beforeEach(({ page }) => {
  page.setDefaultTimeout(15_000);
});
let temporary = '',
  wrapper = '',
  counter = '';
const sameStep = (a: CheckerStep, b: CheckerStep | undefined) =>
  b !== undefined && a.from === b.from && a.to === b.to;
function sql(statement: string) {
  const database = process.env.NEONDOJO_E2E_DATABASE_URL;
  if (
    !database ||
    !/^\/neondojo_e2e_[a-f0-9]{32}$/.test(new URL(database).pathname)
  )
    throw new Error(
      'Full-match fixtures require the Playwright test database.',
    );
  return execFileSync(
    'psql',
    [database, '-X', '-q', '-v', 'ON_ERROR_STOP=1', '-At', '-c', statement],
    { encoding: 'utf8' },
  ).trim();
}
const literal = (value: string) => `'${value.replaceAll("'", "''")}'`;
async function getSession(page: Page, id: string) {
  const response = await page.request.get(`/api/play/sessions/${id}`);
  expect(response.status()).toBe(200);
  return (await response.json()) as PlaySession;
}
async function waitForAutomaticRoll(page: Page, state: PlaySession) {
  const last = state.history.at(-1);
  if (
    state.phase !== 'roll' ||
    last?.notation !== 'pass' ||
    last.player === state.human_player
  )
    return state;
  await expect(page.locator('.play-board')).toHaveAttribute(
    'data-play-phase',
    'move',
    { timeout: 200_000 },
  );
  return getSession(page, state.id);
}
async function action(page: Page, label: string) {
  const waiting = page.waitForResponse(
    (r) => r.url().endsWith('/actions') && r.request().method() === 'POST',
    { timeout: 200_000 },
  );
  if (label === 'Confirm turn')
    await page.locator('[data-dice-action="confirm"]').click();
  else if (label === 'No legal move — pass') await passDice(page).click();
  else if (label === 'Roll dice') await rollDice(page).click();
  else if (label === 'Cube') await doubleCube(page).click();
  else await page.getByRole('button', { name: label, exact: true }).click();
  const response = await waiting;
  expect(response.status(), await response.text()).toBe(200);
  return (await response.json()) as PlaySession;
}
async function show(page: Page, id?: string) {
  if (id)
    await page.evaluate(
      (value) => localStorage.setItem('neondojo.play-session', value),
      id,
    );
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(page.locator('.play-board')).toBeVisible();
}
async function reloadUnchanged(page: Page, session: PlaySession) {
  await show(page, session.id);
  expect(await getSession(page, session.id)).toEqual(session);
  await expect(page.locator('.play-board')).toHaveAttribute(
    'data-direction',
    session.bearoff_direction,
  );
  await expect(page.locator('.play-board')).toHaveAttribute(
    'data-human-player',
    String(session.human_player),
  );
}
async function create(
  page: Page,
  human: number,
  direction: string,
  plies: number,
  length = 3,
  openingSeed?: { rolls?: number[][] },
  fromApi = false,
) {
  const settings = await (await page.request.get('/api/settings')).json();
  const response = await page.request.put('/api/settings', {
    data: {
      ...settings,
      gnubg_path: wrapper,
      play_plies: plies,
      bearoff_direction: direction,
    },
  });
  expect(response.status()).toBe(200);
  if (fromApi) {
    const response = await page.request.post('/api/play/sessions', {
      data: {
        human_player: human,
        match_length: length,
        play_mode: 'competition',
      },
    });
    expect(response.status(), await response.text()).toBe(201);
    return (await response.json()) as PlaySession;
  }
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await page
    .getByRole('radio', { name: human ? 'Black' : 'White', exact: true })
    .check();
  await page
    .getByRole('tabpanel', { name: 'Playing', exact: true })
    .getByRole('radio', { name: 'Competition', exact: true })
    .check();
  await page
    .getByRole('combobox', { name: 'Match length', exact: true })
    .selectOption(String(length));
  const waiting = page.waitForResponse(
    (r) =>
      r.url().endsWith('/api/play/sessions') && r.request().method() === 'POST',
    { timeout: 200_000 },
  );
  const opening = openingSeed ? watchOpening(page) : undefined;
  await page.getByRole('button', { name: 'Start match', exact: true }).click();
  const created = await waiting;
  expect(created.status(), await created.text()).toBe(201);
  const session = (await created.json()) as PlaySession;
  expect(session.engine.plies).toBe(plies);
  expect(session.human_player).toBe(human);
  expect(session.bearoff_direction).toBe(direction);
  const entry = await archivedMatch(page, session);
  expect(entry.position_count).toBe(0);
  archivedMatches.set(session.id, entry.id);
  return opening ? await opening(openingSeed?.rolls) : session;
}
async function checkerTurn(page: Page, state: PlaySession, swap: boolean) {
  let dice = [...state.position.dice];
  if (state.legal_moves.length === 0)
    return await action(page, 'No legal move — pass');
  let turns = orderedLegalTurns(
    state.position.board,
    state.human_player,
    dice,
    state.legal_moves,
  );
  const swapping = page.getByRole('button', { name: /Swap dice order/ });
  const reversed = orderedLegalTurns(
    state.position.board,
    state.human_player,
    [dice[1], dice[0]],
    state.legal_moves,
  );
  if (
    dice[0] !== dice[1] &&
    ((swap && reversed.length > 0) || turns.length === 0)
  ) {
    await swapping.first().click();
    dice = [dice[1], dice[0]];
    turns = orderedLegalTurns(
      state.position.board,
      state.human_player,
      dice,
      state.legal_moves,
    );
  }
  expect(
    turns.length,
    `No ordered native-compatible turn: ${JSON.stringify(state)}`,
  ).toBeGreaterThan(0);
  const chosen = turns[0];
  const prefix: CheckerStep[] = [];
  for (const step of chosen.steps) {
    await expect(page.locator('.play-workspace')).toHaveAttribute(
      'aria-busy',
      'false',
    );
    const candidates = turns.filter((turn) =>
      prefix.every((old, index) => sameStep(old, turn.steps[index])),
    );
    const expected = [
      ...new Set(
        candidates
          .filter((turn) => turn.steps[prefix.length]?.from === step.from)
          .map((turn) => turn.steps[prefix.length].to),
      ),
    ].sort((a, b) => a - b);
    await page
      .locator(`[data-checker-hover-point="${step.from}"]`)
      .last()
      .hover();
    await expect
      .poll(async () =>
        (
          await page
            .locator('circle[data-play-destination]')
            .evaluateAll((nodes) =>
              nodes.map((node) =>
                Number(node.getAttribute('data-play-destination')),
              ),
            )
        ).sort((a, b) => a - b),
      )
      .toEqual(expected);
    if (
      !test.info().attachments.some((item) => item.name === 'destination-hover')
    ) {
      const image = test.info().outputPath('destination-hover.png');
      await page.screenshot({ path: image, fullPage: true });
      await test
        .info()
        .attach('destination-hover', { path: image, contentType: 'image/png' });
    }
    await page
      .locator(`[data-checker-hover-point="${step.from}"]`)
      .last()
      .click();
    prefix.push(step);
    const selected = prefix
      .map(
        (part) =>
          `${part.from === 0 || part.from === 25 ? 'Bar' : relativePoint(part.from, state.human_player)}/${part.to < 0 || part.to > 25 ? 'Bear off' : relativePoint(part.to, state.human_player)}`,
      )
      .join(' ');
    await expect(
      page.locator('.play-selected-steps'),
      `Human ${state.human_player}, dice${dice}, native${state.position.position_id}:${state.position.gnubg_match_id}, chosen${JSON.stringify(chosen.steps)}`,
    ).toHaveText(selected);
  }
  await expect(page.locator('[data-dice-action="confirm"]')).toBeEnabled();
  return await action(page, 'Confirm turn');
}
async function playToEnd(page: Page, initial: PlaySession) {
  let state = initial,
    decisions = 0,
    games = 0,
    reloads = 0;
  const phases = new Set<string>();
  while (state.phase !== 'match_over' && decisions < 1200) {
    state = await waitForAutomaticRoll(page, state);
    phases.add(state.phase);
    if (
      decisions % 13 === 0 ||
      state.phase === 'game_over' ||
      state.phase === 'double_response'
    ) {
      await reloadUnchanged(page, state);
      reloads++;
    }
    if (state.phase === 'roll') {
      await expect(rollDice(page)).toBeVisible();
      if (state.can_double) await expect(doubleCube(page)).toBeEnabled();
      else await expect(doubleCube(page)).toBeDisabled();
      state = await action(
        page,
        state.can_double && decisions % 9 === 0 ? 'Cube' : 'Roll dice',
      );
    } else if (state.phase === 'move')
      state = await checkerTurn(page, state, decisions % 2 === 0);
    else if (state.phase === 'double_response')
      state = await action(page, 'Take double');
    else if (state.phase === 'game_over') {
      state = await finishNextGame(page, () => action(page, 'Next game'));
      games++;
    } else throw new Error(`Unexpected phase ${state.phase}`);
    decisions++;
    if (decisions % 50 === 0 || state.phase === 'game_over')
      console.info('Native browser match progress', {
        human: state.human_player,
        direction: state.bearoff_direction,
        decisions,
        game: state.game_no,
        scores: state.scores,
        phase: state.phase,
      });
  }
  expect(state.phase).toBe('match_over');
  expect(Math.max(...state.scores)).toBeGreaterThanOrEqual(state.match_length);
  expect(state.winner).not.toBeNull();
  expect(reloads).toBeGreaterThan(2);
  expect(state.history.length).toBeGreaterThan(30);
  await reloadUnchanged(page, state);
  const sgf = await (
    await page.request.get(`/api/play/sessions/${state.id}/sgf`)
  ).text();
  const entry = await archivedMatch(page, state);
  expect(entry.id).toBe(archivedMatches.get(state.id));
  expect(entry.result_state).toBe('completed');
  expect(entry.position_count).toBeGreaterThan(30);
  const source = await (
    await page.request.get(`/api/matches/${entry.id}/source`)
  ).text();
  expect(source).toContain(`BTSESSION[${state.id}]`);
  expect(source).toContain('RE[');
  expect(source).toContain(`${gnubgLevel(state.engine.plies)}C 0 1 0.000000`);
  // Leave the mandatory match-result dialog via a real page reload before
  // browsing the archive; background tabs remain correctly modal-blocked.
  await page.reload();
  await page.getByRole('tab', { name: 'Match database', exact: true }).click();
  await page
    .locator('.database-sidebar')
    .getByRole('button', { name: /Bot Matches/ })
    .click();
  const row = page.locator(`#library-row-${entry.id}`);
  await expect(row).toBeVisible();
  await row.getByRole('checkbox').check();
  await expect(
    page.locator('.library-batch-bar').getByRole('button', { name: 'Resume match', exact: true }),
  ).toBeDisabled();
  expect(sgf).toContain('RE[');
  expect(sgf).toContain(`${gnubgLevel(state.engine.plies)}C 0 1 0.000000`);
  writeFileSync(
    test.info().outputPath(`full-match-${state.bearoff_direction}.sgf`),
    sgf,
  );
  await page.screenshot({
    path: test.info().outputPath('match-finished.png'),
    fullPage: true,
  });
  await test.info().attach(`full-match-${state.bearoff_direction}.sgf`, {
    body: sgf,
    contentType: 'application/x-go-sgf',
  });
  await test.info().attach('native-summary', {
    body: JSON.stringify({
      id: state.id,
      decisions,
      games: games + 1,
      reloads,
      phases: [...phases],
      scores: state.scores,
      plies: state.engine.plies,
    }),
    contentType: 'application/json',
  });
  return state;
}

test.beforeAll(async ({ request }) => {
  sql('SELECT 1');
  const real = process.env.GNUBG_BIN;
  if (!real?.startsWith('/'))
    throw new Error('GNUBG_BIN must be the absolute real GNUbg executable');
  temporary = mkdtempSync(join(tmpdir(), 'neondojo-full-native-'));
  wrapper = join(temporary, 'seeded-gnubg');
  counter = join(temporary, 'seed');
  writeFileSync(counter, '1');
  // Configuration validation runs real engine probes too; only gameplay
  // invocations consume deterministic seeds, independently of profile writes.
  writeFileSync(
    wrapper,
    `#!/usr/bin/env python3\nimport os,sys,pathlib\nreal=${JSON.stringify(real)}\nif '--version' in sys.argv or not pathlib.Path('request.json').exists(): os.execv(real,[real]+sys.argv[1:])\np=pathlib.Path(${JSON.stringify(counter)})\nseed=int(p.read_text());p.write_text(str(seed+1))\npathlib.Path('seeded.txt').write_text('set seed '+str(seed)+'\\n'+pathlib.Path('commands.txt').read_text())\nos.execv(real,[real,'-t','-q','-r','-c','seeded.txt'])\n`,
  );
  chmodSync(wrapper, 0o755);
  const settings = await (await request.get('/api/settings')).json();
  expect(
    (
      await request.put('/api/settings', {
        data: { ...settings, gnubg_path: real },
      })
    ).status(),
  ).toBe(200);
  expect((await request.get('/api/health')).status()).toBe(200);
});
test.afterAll(async ({ request }) => {
  if (temporary) {
    const settings = await (await request.get('/api/settings')).json();
    await request.put('/api/settings', {
      data: { ...settings, gnubg_path: process.env.GNUBG_BIN },
    });
    rmSync(temporary, { recursive: true, force: true });
  }
});
test.afterEach(async ({ page }, info) => {
  if (info.status !== info.expectedStatus)
    await info.attach('native-play-failure', {
      body: await page.screenshot({
        fullPage: true,
        path: info.outputPath('native-play-failure.png'),
      }),
      contentType: 'image/png',
    });
});
for (const config of [
  {
    seed: 3,
    human: 1,
    rolls: [
      [1, 1],
      [4, 5],
    ],
  },
  { seed: 8, human: 1, rolls: [[6, 4]] },
  {
    seed: 12,
    human: 0,
    rolls: [
      [3, 3],
      [6, 6],
      [2, 1],
    ],
  },
]) {
  test(`visible native opening seed ${config.seed} with reload`, async ({
    page,
  }) => {
    test.setTimeout(200_000);
    writeFileSync(counter, String(config.seed));
    const state = await create(page, config.human, 'right', 0, 7, config);
    await reloadUnchanged(page, state);
    expect(['move', 'roll', 'double_response']).toContain(state.phase);
  });
}

for (const config of [
  { human: 0, direction: 'right', plies: 3, theme: 'forest' },
  { human: 1, direction: 'left', plies: 1, theme: 'bgblitz:bg-earth' },
]) {
  test(`complete native match with per-die hover ${config.human}/${config.direction}/${config.plies}ply`, async ({
    page,
  }) => {
    test.setTimeout(1_200_000);
    await page.goto('/');
    await expect(page.getByRole('button', { name: /^Profile:/ })).toBeVisible({
      timeout: 30_000,
    });
    const response = await page.request.get('/api/configuration');
    expect(response.status()).toBe(200);
    const profile = await response.json();
    const configuration = profile.current_configuration;
    configuration.appearance.board_theme = config.theme;
    const saved = await page.request.put('/api/configuration', {
      data: { expected_revision: profile.revision, configuration },
    });
    expect(saved.status(), await saved.text()).toBe(200);
    writeFileSync(counter, '1');
    const initial = await create(
      page,
      config.human,
      config.direction,
      config.plies,
      3,
      {},
    );
    await expect(page.locator('.play-board .board-shell')).toHaveClass(
      config.theme.startsWith('bgblitz:') ? /theme-bgblitz/ : /theme-forest/,
    );
    await playToEnd(page, initial);
  });
}

function seedCubeRace(session: PlaySession) {
  const human = session.human_player;
  const board = Array<number>(26).fill(0);
  board[human === 0 ? 6 : 19] = human === 0 ? 1 : -1;
  board[human === 0 ? 24 : 1] = human === 0 ? -2 : 2;
  const key = Buffer.from('MIGkAAAAAAAE', 'base64');
  if (human === 1) {
    key[0] ^= 1 << 6;
    key[1] ^= 1 << 3;
  }
  const matchID = key.toString('base64');
  const nativeID = `AwAAAAEAAAAAAA:${matchID}`;
  const state = {
    ...session,
    phase: 'move',
    can_double: false,
    legal_moves: [
      {
        notation: '6/2',
        steps: [{ from: human === 0 ? 6 : 19, to: human === 0 ? 2 : 23 }],
      },
    ],
    history: [],
    scores: [0, 0],
    game_no: 1,
    winner: null,
    position: {
      ...session.position,
      player: human,
      board,
      dice: [1, 1],
      position_id: 'AwAAAAEAAAAAAA',
      gnubg_match_id: matchID,
      ply: 0,
    },
  };
  const setup = human === 0 ? 'AW[f]AB[x][x]' : 'AW[a][a]AB[s]';
  const side = human === 0 ? 'W' : 'B';
  const sgf = `(;FF[4]GM[6]MI[length:5][game:0][ws:0][bs:0]PW[Human]PB[GNUbg]RU[Crawford];PL[${side}]DI[11];PL[W]AE[a:y]${setup};PL[${side}])`;
  if (!/^[0-9a-f-]{36}$/.test(session.id))
    throw new Error('Invalid fixture session UUID');
  sql(
    `UPDATE play_sessions SET state=${literal(JSON.stringify(state))}::jsonb,sgf=${literal(sgf)},native_id=${literal(nativeID)} WHERE id=${literal(session.id)}::uuid`,
  );
}
for (const config of [
  {
    human: 0,
    direction: 'right',
    plies: 3,
    response: 'Take double',
    points: 2,
  },
  { human: 1, direction: 'left', plies: 1, response: 'Pass double', points: 1 },
  {
    human: 0,
    direction: 'right',
    plies: 2,
    response: 'Pass double',
    points: 1,
  },
]) {
  test(`native bot cube/${config.response}/${config.human}/${config.direction}/${config.plies}ply with reload and next game`, async ({
    page,
  }) => {
    test.setTimeout(300_000);
    const created = await create(
      page,
      config.human,
      config.direction,
      config.plies,
      5,
      undefined,
      true,
    );
    seedCubeRace(created);
    await page.goto('/');
    await show(page, created.id);
    let state = await getSession(page, created.id);
    state = await checkerTurn(page, state, false);
    expect(state.phase).toBe('double_response');
    expect(state.history.at(-1)).toMatchObject({
      player: 1 - config.human,
      notation: 'double',
    });
    await reloadUnchanged(page, state);
    state = await action(page, config.response);
    expect(state.phase).toBe('game_over');
    expect(state.scores[1 - config.human]).toBe(config.points);
    expect(state.winner).toBe(1 - config.human);
    await reloadUnchanged(page, state);
    writeFileSync(counter, '1');
    state = await finishNextGame(page, () => action(page, 'Next game'));
    state = await waitForAutomaticRoll(page, state);
    expect(state.game_no).toBe(2);
    expect(state.engine.plies).toBe(config.plies);
    await reloadUnchanged(page, state);
    const marker = `${gnubgLevel(config.plies)}C 0 1 0.000000`;
    let sgf = await (
      await page.request.get(`/api/play/sessions/${state.id}/sgf`)
    ).text();
    for (let i = 0; i < 10 && !sgf.split('(;').at(-1)!.includes(marker); i++) {
      state = await waitForAutomaticRoll(page, state);
      if (state.phase === 'roll') state = await action(page, 'Roll dice');
      else if (state.phase === 'move')
        state = await checkerTurn(page, state, i % 2 === 0);
      else if (state.phase === 'double_response')
        state = await action(page, 'Take double');
      else break;
      sgf = await (
        await page.request.get(`/api/play/sessions/${state.id}/sgf`)
      ).text();
    }
    expect(
      sgf.split('(;').at(-1),
      `Native next-game bot search depth must be actual${gnubgLevel(config.plies)}C, not only metadata`,
    ).toContain(marker);
    await test
      .info()
      .attach(`native-cube-${config.human}-${config.plies}.sgf`, {
        body: sgf,
        contentType: 'application/x-go-sgf',
      });
  });
}
