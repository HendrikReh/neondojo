import { rollDice } from './play-board-actions';
import { test, expect, type Page } from './test';
import type { PlaySession } from '../src/play-api';
declare global {
  interface Window {
    __playAudio: { duration: number; power: number }[];
    __playContexts: BaseAudioContext[];
    __decodedRecordings: number;
  }
}
const id = 'd4a5d25c-023e-4fdb-8737-5630839c4861';
function fixture(human = 0): PlaySession {
  const board = Array<number>(26).fill(0);
  board[6] = 2;
  board[1] = -1;
  return {
    id,
    version: 0,
    human_player: human,
    human_name: 'Player',
    match_length: 7,
    bearoff_direction: human === 0 ? 'right' : 'left',
    engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 2 },
    position: {
      id,
      match_uuid: id,
      ordinal: 0,
      game_no: 1,
      ply: 0,
      player: human,
      dice: [2, 1],
      board,
      position_id: '4HPwATDgc/ABMA',
      gnubg_match_id: 'cInlAAAAAAAE',
      notation: '',
    },
    phase: 'move',
    can_double: false,
    legal_moves: [
      {
        notation: '6/4 6/5',
        steps: [
          { from: 6, to: 4 },
          { from: 6, to: 5 },
        ],
      },
    ],
    history: [],
    scores: [0, 0],
    game_no: 1,
    winner: null,
    opening: null,
  };
}
async function install(
  page: Page,
  value: PlaySession,
  action?: (data: { action: string }) => PlaySession | null,
) {
  await page.addInitScript((sessionId) => {
    localStorage.setItem('bgtutor.play-session', sessionId);
    window.__playAudio = [];
    window.__playContexts = [];
    window.__decodedRecordings = 0;
    const decode = AudioContext.prototype.decodeAudioData;
    AudioContext.prototype.decodeAudioData = function (data) {
      return decode.call(this, data).then((buffer) => {
        ++window.__decodedRecordings;
        return buffer;
      });
    };
    const original = AudioBufferSourceNode.prototype.start;
    AudioBufferSourceNode.prototype.start = function (...args) {
      const data = this.buffer?.getChannelData(0);
      if (data) {
        window.__playAudio.push({
          duration: this.buffer!.duration,
          power: data.reduce((sum, n) => sum + n * n, 0) / data.length,
        });
        window.__playContexts.push(this.context);
      }
      return original.apply(this, args);
    };
  }, id);
  let saved = value;
  await page.route('**/api/play/sessions', (route) =>
    route.fulfill({ json: [saved] }),
  );
  await page.route(`**/api/play/sessions/${id}`, (route) =>
    route.fulfill({ json: saved }),
  );
  await page.route(`**/api/play/sessions/${id}/actions`, (route) => {
    const next = action?.(route.request().postDataJSON());
    if (!next)
      return route.fulfill({
        status: 503,
        json: { error: 'Native test failure' },
      });
    saved = next;
    return route.fulfill({ json: saved });
  });
  await page.goto('/');
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(page.locator('.play-board')).toBeVisible();
}
const heard = (page: Page) =>
  page.evaluate(() =>
    window.__playAudio.map((sound) => Math.round(sound.duration * 100) / 100),
  );
async function step(page: Page, from: number, to: number) {
  const checker = page.locator(`[data-checker-hover-point="${from}"]`).last();
  await checker.hover();
  await expect(
    page.locator(`circle[data-play-destination="${to}"]`),
  ).toBeVisible();
  await checker.click();
  await expect(page.locator('.play-board')).toHaveAttribute(
    'data-play-animation',
    'idle',
  );
}
test('checker steps sound immediately; bot dice/hits use native history; reload is silent', async ({
  page,
}) => {
  const saved = fixture();
  const after: PlaySession = {
    ...saved,
    version: 1,
    phase: 'roll',
    legal_moves: [],
    history: [
      { player: 0, dice: [2, 1], notation: '6/4 6/5' },
      { player: 1, dice: [3, 1], notation: '24/21 21/20' },
    ],
  };
  await install(page, saved, () => after);
  await page.locator('[data-checker-hover-point="6"]').last().hover();
  expect(await heard(page)).toEqual([]);
  await step(page, 6, 4);
  await step(page, 6, 5);
  expect(await heard(page)).toEqual([0.41, 0.41]);
  await page
    .locator('[data-play-die-index="0"][data-dice-action="confirm"]')
    .click();
  await expect.poll(() => heard(page)).toEqual([0.41, 0.41, 0.28, 0.05, 0.05]);
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.__playContexts.every((ctx) => ctx.state === 'running'),
      ),
    )
    .toBe(true);
  expect(
    await page.evaluate(() =>
      window.__playAudio.every((sound) => sound.power > 0.000025),
    ),
  ).toBe(true);
  expect(await page.evaluate(() => window.__decodedRecordings)).toBe(4);
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(rollDice(page)).toBeVisible();
  expect(await heard(page)).toEqual([]);
});
for (const human of [0, 1])
  test(`distinct hit and bear-off for color ${human}`, async ({ page }) => {
    const saved = fixture(human),
      from = human === 0 ? 3 : 22,
      to = human === 0 ? 1 : 24,
      off = human === 0 ? -1 : 26;
    saved.position.board = Array<number>(26).fill(0);
    saved.position.board[from] = human === 0 ? 1 : -1;
    saved.position.board[to] = human === 0 ? -1 : 1;
    saved.legal_moves = [
      {
        notation: '3/1 1/off',
        steps: [
          { from, to },
          { from: to, to: off },
        ],
      },
    ];
    await install(page, saved);
    await step(page, from, to);
    await step(page, to, off);
    expect(await heard(page)).toEqual([0.05, 0.41]);
    await page.getByRole('button', { name: 'Undo step', exact: true }).click();
    await page.getByRole('button', { name: 'Undo step', exact: true }).click();
    expect(await heard(page)).toEqual([0.05, 0.41]);
  });
for (const opening of [false, true])
  test(`${opening ? 'opening' : 'normal'} dice sound once; acknowledgment is silent`, async ({
    page,
  }) => {
    const before = fixture();
    before.phase = opening ? 'opening_roll' : 'roll';
    before.position.dice = [];
    before.legal_moves = [];
    const result: PlaySession = {
      ...before,
      version: 1,
      phase: opening ? 'opening_result' : 'move',
      position: { ...before.position, dice: [6, 4] },
      opening: opening
        ? {
            white_die: 6,
            black_die: 4,
            winner: 0,
            rolls: [
              [2, 2],
              [6, 4],
            ],
          }
        : null,
    };
    await install(page, before, (data) =>
      data.action === 'start_opening_move'
        ? { ...result, version: 2, phase: 'move' }
        : result,
    );
    await (
      opening
        ? page.getByRole('button', { name: 'Roll one die each', exact: true })
        : rollDice(page)
    ).click();
    await expect(page.locator('.play-board')).toHaveAttribute(
      'data-play-animation',
      'idle',
    );
    await expect
      .poll(() => heard(page))
      .toEqual(opening ? [0.28, 0.28] : [0.28]);
    if (opening) {
      await page
        .getByRole('button', { name: 'Begin the first move', exact: true })
        .click();
      await expect(page.locator('.play-opening')).toHaveCount(0);
      expect(await heard(page)).toEqual([0.28, 0.28]);
    }
  });
test('sound setting cancels, saves, survives reload and applies to ongoing games', async ({
  page,
}) => {
  let settings = {
    version: 1,
    player_name: 'Player',
    analysis_engine: 'gnubg',
    analysis_plies: 2,
    play_engine: 'gnubg',
    play_plies: 1,
    gnubg_path: '/test/gnubg',
    bearoff_direction: 'right',
  };
  await page.route('**/api/settings', (route) => {
    if (route.request().method() === 'PUT')
      settings = {
        ...route.request().postDataJSON(),
        version: settings.version + 1,
      };
    return route.fulfill({ json: settings });
  });
  await install(page, fixture());
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  const open = async () => {
    await page
      .getByRole('button', { name: 'Settings', exact: true })
      .first()
      .click();
    await dialog.getByRole('button', { name: 'Playing', exact: true }).click();
  };
  await open();
  await expect(dialog.getByLabel('Game sounds', { exact: true })).toBeChecked();
  await dialog.getByLabel('Game sounds', { exact: true }).uncheck();
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await open();
  await expect(dialog.getByLabel('Game sounds', { exact: true })).toBeChecked();
  await dialog.getByLabel('Game sounds', { exact: true }).uncheck();
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  expect(
    await page.evaluate(() => localStorage.getItem('bgtutor.play-sounds')),
  ).toBe('off');
  const configuration = await page.evaluate(async () => {
    const response = await fetch('/api/configuration');
    return response.json();
  });
  expect(configuration.current_configuration.general.play_sounds).toBe(false);
  expect(configuration.active_profile.configuration.general.play_sounds).toBe(
    true,
  );
  await page.reload();
  await page.getByRole('tab', { name: 'Playing', exact: true }).click();
  await expect(page.locator('.play-board')).toBeVisible();
  await step(page, 6, 4);
  expect(await heard(page)).toEqual([]);
  await open();
  await expect(
    dialog.getByLabel('Game sounds', { exact: true }),
  ).not.toBeChecked();
  await dialog.getByLabel('Game sounds', { exact: true }).check();
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await step(page, 6, 5);
  await expect.poll(() => heard(page)).toEqual([0.41]);
});
test('unavailable audio never blocks legal play or engine error handling', async ({
  page,
}) => {
  await page.addInitScript(() =>
    Object.defineProperty(window, 'AudioContext', {
      value: class {
        constructor() {
          throw new Error('No audio device');
        }
      },
    }),
  );
  await install(page, fixture());
  await step(page, 6, 4);
  await step(page, 6, 5);
  await expect(
    page.locator('[data-play-die-index="0"][data-dice-action="confirm"]'),
  ).toBeEnabled();
  await page
    .locator('[data-play-die-index="0"][data-dice-action="confirm"]')
    .click();
  await expect(page.getByRole('alert')).toBeVisible();
  expect(await heard(page)).toEqual([]);
});

test('a rejected browser audio resume drops cues without queuing them for later', async ({
  page,
}) => {
  await page.addInitScript(() => {
    const Native = window.AudioContext;
    window.AudioContext = class extends Native {
      get state(): AudioContextState {
        return 'suspended';
      }
      resume(): Promise<void> {
        return Promise.reject(new Error('Playback blocked'));
      }
    };
  });
  await install(page, fixture());
  await step(page, 6, 4);
  await expect(page.locator('.play-selected-steps')).toHaveText('6/4');
  expect(await heard(page)).toEqual([]);
});

test('failed dice actions never play a roll cue', async ({ page }) => {
  const before = fixture();
  before.phase = 'roll';
  before.position.dice = [];
  before.legal_moves = [];
  await install(page, before);
  await rollDice(page).click();
  await expect(page.getByRole('alert')).toBeVisible();
  expect(await heard(page)).toEqual([]);
});

test('missing recording is silent without blocking moves and retries on the next gesture', async ({
  page,
}) => {
  let missing = true;
  await page.route('**/*checker-move*.wav*', (route) => {
    if (missing && route.request().resourceType() === 'fetch')
      return route.abort();
    return route.continue();
  });
  await install(page, fixture());
  await step(page, 6, 4);
  expect(await heard(page)).toEqual([]);
  await expect(page.locator('.play-selected-steps')).toHaveText('6/4');
  missing = false;
  await step(page, 6, 5);
  await expect.poll(() => heard(page)).toEqual([0.41]);
  await expect(page.getByRole('alert')).toHaveCount(0);
});

test('muting while a recording loads prevents delayed playback after decode', async ({
  page,
}) => {
  let release: () => void = () => {};
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.route('**/*checker-move*.wav*', async (route) => {
    if (route.request().resourceType() === 'fetch') await gate;
    return route.continue();
  });
  let settings = {
    version: 1,
    player_name: 'Player',
    analysis_engine: 'gnubg',
    analysis_plies: 2,
    play_engine: 'gnubg',
    play_plies: 1,
    gnubg_path: '/test/gnubg',
    bearoff_direction: 'right',
  };
  await page.route('**/api/settings', (route) => {
    if (route.request().method() === 'PUT')
      settings = {
        ...route.request().postDataJSON(),
        version: settings.version + 1,
      };
    return route.fulfill({ json: settings });
  });
  await install(page, fixture());
  await step(page, 6, 4);
  expect(await heard(page)).toEqual([]);
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Settings', exact: true });
  await dialog.getByRole('button', { name: 'Playing', exact: true }).click();
  await dialog.getByLabel('Game sounds', { exact: true }).uncheck();
  await dialog.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  release();
  await expect
    .poll(() => page.evaluate(() => window.__decodedRecordings))
    .toBe(4);
  expect(await heard(page)).toEqual([]);
  await expect(page.locator('.play-selected-steps')).toHaveText('6/4');
});

for (const muted of [false, true])
  test(`bot cube offer sounds once and respects mute=${muted}`, async ({
    page,
  }) => {
    await page.addInitScript(
      (muted) =>
        localStorage.setItem('bgtutor.play-sounds', muted ? 'off' : 'on'),
      muted,
    );
    const before = fixture();
    const after: PlaySession = {
      ...before,
      version: 1,
      phase: 'double_response',
      legal_moves: [],
      position: { ...before.position, dice: [] },
      history: [
        { player: 0, dice: [2, 1], notation: '6/4 6/5' },
        { player: 1, dice: [], notation: 'double' },
      ],
    };
    await install(page, before, () => after);
    await step(page, 6, 4);
    await step(page, 6, 5);
    await page
      .locator('[data-play-die-index="0"][data-dice-action="confirm"]')
      .click();
    await expect(
      page.getByRole('alertdialog', { name: 'Cube decision', exact: true }),
    ).toBeVisible();
    await expect
      .poll(() => heard(page))
      .toEqual(muted ? [] : [0.41, 0.41, 0.15]);
    await page.reload();
    await page.locator('#workspace-tab-play').click();
    await expect(
      page.getByRole('alertdialog', { name: 'Cube decision', exact: true }),
    ).toBeVisible();
    expect(await heard(page)).toEqual([]);
  });
