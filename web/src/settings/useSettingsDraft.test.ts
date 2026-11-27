import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import {
  defaultConfiguration,
  defaultErrorThresholds,
} from './configuration/configuration-model';
import type { AppSettings, EngineInfo } from './settings-api';
import { archiveThemesPreloadKey, themeStorageKey } from './ui-preferences';

const hooks = vi.hoisted(() => ({
  slots: [] as unknown[],
  index: 0,
  pending: new Map<number, () => (() => void) | void>(),
}));

interface EffectSlot {
  deps: unknown[];
  cleanup?: (() => void) | void;
}

const mocks = vi.hoisted(() => ({
  loadSettings: vi.fn(),
  saveSettings: vi.fn(),
  loadEngines: vi.fn(),
  checkGnubg: vi.fn(),
  configuration: null as null | {
    current: typeof defaultConfiguration;
    ready: boolean;
    generation: number;
    refresh: ReturnType<typeof vi.fn>;
    applyCurrent: ReturnType<typeof vi.fn>;
    reloadServer: ReturnType<typeof vi.fn>;
  },
  sage: {
    installation: { version: 1, python_path: '/opt/python' },
    path: '/opt/python',
    setPath: vi.fn(),
    busy: false,
    error: undefined as unknown,
    dirty: false,
    verify: vi.fn(async () => {}),
    reload: vi.fn(),
    discard: vi.fn(),
  },
  myProfile: {
    changed: false,
    save: vi.fn(async () => {}),
    discard: vi.fn(),
  },
}));

vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useInsertionEffect(effect: () => void) {
    effect();
  },
  useState(initial: unknown) {
    const index = hooks.index++;
    if (!(index in hooks.slots))
      hooks.slots[index] = typeof initial === 'function' ? initial() : initial;
    return [
      hooks.slots[index],
      (next: unknown) => {
        hooks.slots[index] =
          typeof next === 'function' ? next(hooks.slots[index]) : next;
      },
    ];
  },
  useRef(initial: unknown) {
    return (hooks.slots[hooks.index++] ??= { current: initial });
  },
  useCallback(fn: unknown, deps: unknown[]) {
    const index = hooks.index++;
    const prior = hooks.slots[index] as
      { fn: unknown; deps: unknown[] } | undefined;
    if (!prior || deps.some((value, i) => !Object.is(value, prior.deps[i]))) {
      hooks.slots[index] = { fn, deps };
      return fn;
    }
    return prior.fn;
  },
  useEffect(effect: () => (() => void) | void, deps: unknown[]) {
    const index = hooks.index++;
    const prior = hooks.slots[index] as EffectSlot | undefined;
    if (!prior || deps.some((value, i) => !Object.is(value, prior.deps[i]))) {
      hooks.slots[index] = { deps, cleanup: prior?.cleanup };
      hooks.pending.set(index, effect);
    }
  },
}));

vi.mock('./configuration/ConfigurationProvider', () => ({
  useOptionalConfiguration: () => mocks.configuration,
}));
vi.mock('./OpenSageInstallation', () => ({
  useOpenSageInstallation: () => mocks.sage,
}));
vi.mock('./MyPlayerProfileSetting', () => ({
  useMyPlayerProfileDraft: () => mocks.myProfile,
}));
vi.mock('../shared/i18n/i18n', async (original) => ({
  ...(await original<typeof import('../shared/i18n/i18n')>()),
  useI18n: () => ({ locale: 'en' as const }),
}));
vi.mock('./settings-api', () => ({
  loadSettings: mocks.loadSettings,
  saveSettings: mocks.saveSettings,
  loadEngines: mocks.loadEngines,
  checkGnubg: mocks.checkGnubg,
}));

import { useSettingsDraft } from './useSettingsDraft';
import { HttpError } from '../shared/http/api';

const baseSettings: AppSettings = {
  version: 3,
  player_name: 'Alice',
  analysis_engine: 'gnubg',
  analysis_plies: 2,
  play_engine: 'gnubg',
  play_plies: 2,
  play_top_two: false,
  gnubg_path: '/opt/gnubg',
  gnubg_threads: 4,
  gnubg_play_threads: 2,
  open_sage_threads: null,
  open_sage_play_threads: null,
  parallel_analysis_jobs: 2,
  bearoff_direction: 'left',
  default_play_mode: 'competition',
  tutor_threshold: 0.08,
  training_auto_analysis: false,
};

const baseEngines: EngineInfo[] = [
  {
    id: 'gnubg',
    name: 'GNU Backgammon',
    available: true,
    version: 'GNUbg 1.08',
    plies: [1, 2, 3, 4, 5],
    max_parallel_threads: 10,
  },
];

function commit() {
  for (const [index, effect] of hooks.pending) {
    const slot = hooks.slots[index] as EffectSlot;
    slot.cleanup?.();
    slot.cleanup = effect();
  }
  hooks.pending.clear();
}

function settle() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

let storageMap: Map<string, string>;

beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
  mocks.loadSettings.mockReset().mockResolvedValue(baseSettings);
  mocks.saveSettings
    .mockReset()
    .mockImplementation(async (next: AppSettings) => ({
      ...next,
      version: next.version + 1,
    }));
  mocks.loadEngines.mockReset().mockResolvedValue(baseEngines);
  mocks.checkGnubg.mockReset().mockResolvedValue({ version: 'GNUbg 1.08' });
  mocks.configuration = null;
  mocks.sage.dirty = false;
  mocks.sage.busy = false;
  mocks.sage.verify.mockClear();
  mocks.sage.discard.mockClear();
  mocks.myProfile.changed = false;
  mocks.myProfile.save.mockClear();
  mocks.myProfile.discard.mockClear();
  storageMap = new Map();
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => storageMap.get(key) ?? null,
    setItem: (key: string, value: string) => storageMap.set(key, value),
    removeItem: (key: string) => storageMap.delete(key),
  });
});

afterEach(() => {
  for (const slot of hooks.slots) {
    if (slot && typeof slot === 'object' && 'cleanup' in slot) {
      (slot as EffectSlot).cleanup?.();
    }
  }
  vi.unstubAllGlobals();
});

function renderDraft(
  overrides: Partial<Parameters<typeof useSettingsDraft>[0]> = {},
) {
  hooks.index = 0;
  return useSettingsDraft({
    open: true,
    inProfilesSection: false,
    theme: 'forest',
    appearance: { theme: 'graphite', mode: 'light' },
    onApply: vi.fn(),
    onReloadCleared: vi.fn(),
    ...overrides,
  });
}

it('loads settings and engines, tracks dirty hints, and discards threshold edits cleanly', async () => {
  let state = renderDraft();
  commit();
  await settle();

  state = renderDraft();
  expect(state.loading).toBe(false);
  expect(state.draft?.player_name).toBe('Alice');
  expect(state.dirty).toBe(false);
  expect(state.thresholdFields).toEqual({
    questionable: defaultErrorThresholds.questionable.toFixed(3),
    error: defaultErrorThresholds.error.toFixed(3),
    blunder: defaultErrorThresholds.blunder.toFixed(3),
    large_blunder: defaultErrorThresholds.large_blunder.toFixed(3),
  });

  // Editing thresholds and GNUbg path marks both configuration and installation dirty.
  state.setThresholdFields({
    ...state.thresholdFields,
    blunder: '0.095',
  });
  state.update({ gnubg_path: '/usr/local/bin/gnubg' });
  state = renderDraft();
  expect(state.dirty).toBe(true);
  expect(state.gnubgEdited).toBe(true);
  expect(state.canApply).toBe(true);
  expect(state.applyHint).toContain('current configuration');
  expect(state.applyHint).toContain('this computer');

  // Discarding restores thresholds as well as draft fields and delegates to child drafts.
  state.discardDraft();
  state = renderDraft();
  expect(state.dirty).toBe(false);
  expect(state.draft?.gnubg_path).toBe('/opt/gnubg');
  expect(state.thresholdFields.blunder).toBe(
    defaultErrorThresholds.blunder.toFixed(3),
  );
  expect(mocks.sage.discard).toHaveBeenCalledTimes(1);
  expect(mocks.myProfile.discard).toHaveBeenCalledTimes(1);
});

it('rejects non-increasing error thresholds before writing to the server', async () => {
  renderDraft();
  commit();
  await settle();

  let state = renderDraft();
  state.setThresholdFields({
    questionable: '0.050',
    error: '0.040',
    blunder: '0.080',
    large_blunder: '0.160',
  });
  state = renderDraft();
  expect(await state.applyDraft()).toBe(false);
  expect(mocks.saveSettings).not.toHaveBeenCalled();

  state = renderDraft();
  expect(state.saveError).toBeInstanceOf(Error);
  expect((state.saveError as Error).message).toContain('strictly increasing');
});

it('checks both analysis and play GNUbg thread counts and persists mixed BGBlitz preload hints on apply', async () => {
  const onApply = vi.fn();
  renderDraft({ onApply });
  commit();
  await settle();

  let state = renderDraft({ onApply });
  await state.checkInstallation();
  expect(mocks.checkGnubg.mock.calls.map((call) => call[2])).toEqual([4, 2]);
  state = renderDraft({ onApply });
  expect(state.check).toEqual({ path: '/opt/gnubg', version: 'GNUbg 1.08' });

  // Pair a built-in board with a BGBlitz checker theme and apply.
  state.setDraftBoard({
    theme: 'forest',
    checkerTheme: 'bgblitz:x22',
    diceTheme: null,
    cubeTheme: null,
  });
  state = renderDraft({ onApply });
  expect(await state.applyDraft()).toBe(true);

  expect(storageMap.get(themeStorageKey)).toBe('forest');
  expect(storageMap.get(archiveThemesPreloadKey)).toBe('1');
  expect(onApply).toHaveBeenCalledTimes(1);
});

it('surfaces 409 conflicts and reloads the latest server state via reloadLatest', async () => {
  const onReloadCleared = vi.fn();
  const reloadServer = vi.fn(async () => {});
  mocks.configuration = {
    current: structuredClone(defaultConfiguration),
    ready: true,
    generation: 1,
    refresh: vi.fn(async () => {}),
    applyCurrent: vi.fn(async () => structuredClone(defaultConfiguration)),
    reloadServer,
  };
  mocks.saveSettings.mockRejectedValueOnce(
    new HttpError(409, 'Version changed'),
  );

  renderDraft({ onReloadCleared });
  commit();
  await settle();

  let state = renderDraft({ onReloadCleared });
  state.update({ gnubg_path: '/opt/conflict-gnubg' });
  state = renderDraft({ onReloadCleared });
  expect(await state.applyDraft()).toBe(false);

  state = renderDraft({ onReloadCleared });
  expect(state.conflict).toBe(true);
  expect(state.canApply).toBe(false);

  await state.reloadLatest();
  expect(reloadServer).toHaveBeenCalledTimes(1);
  expect(onReloadCleared).toHaveBeenCalledTimes(1);
});
