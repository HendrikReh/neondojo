import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { MouseEvent } from 'react';
import { AppError } from '../shared/i18n/i18n-core';
import type { Job, Match, Position } from '../shared/http/api';
import { PAUSED_MATCHES_DATABASE_ID } from '../workspaces/library/library-api';

const hooks = vi.hoisted(() => ({
  slots: [] as unknown[],
  index: 0,
  pending: new Map<number, () => (() => void) | void>(),
  request: vi.fn(),
  cancelRestoration: vi.fn(),
  clearSelection: vi.fn(),
  readSelection: vi.fn(() => undefined as unknown),
  navigation: {
    match: undefined as Match | undefined,
    position: undefined as Position | undefined,
    game: undefined as { game_no: number } | undefined,
    gameLoaded: false,
    loading: false,
    error: undefined as unknown,
    intentValue: 1,
    intent: vi.fn(() => hooks.navigation.intentValue),
    openMatch: vi.fn(async () => {}),
    selectMatch: vi.fn(async () => {}),
    openPosition: vi.fn(async () => {}),
    cancel: vi.fn(),
    clear: vi.fn(),
    clearError: vi.fn(),
    invalidateMatchAnalysis: vi.fn(),
  },
}));

interface EffectSlot {
  deps: unknown[];
  cleanup?: (() => void) | void;
}

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

vi.mock('../settings/configuration/ConfigurationProvider', () => ({
  useWorkspaceDefaults: () => undefined,
}));
vi.mock('../shared/http/api', () => ({
  request: hooks.request,
}));
vi.mock('./useGameNavigation', () => ({
  useGameNavigation: () => hooks.navigation,
}));
vi.mock('./usePositionKeys', () => ({
  usePositionKeys: vi.fn(),
}));
vi.mock('./useSelectionRestore', () => ({
  useSelectionRestore: () => ({
    restoring: false,
    cancelRestoration: hooks.cancelRestoration,
  }),
}));
vi.mock('./workspace-state', () => ({
  clearSelection: hooks.clearSelection,
  readSelection: hooks.readSelection,
}));

import { useAppNavigation, type AppView } from './useAppNavigation';

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

const sampleMatch = {
  id: 'match-1',
  match_no: 1,
  database_id: 'db-1',
  filename: 'game.sgf',
  display_name: 'Game 1',
  format: 'sgf',
  white: 'Ada',
  black: 'Bob',
  match_length: 7,
  game_count: 1,
  position_count: 10,
  created_at: '2026-11-01T00:00:00Z',
} as unknown as Match;

let replaceState: ReturnType<typeof vi.fn>;
let scrollTo: ReturnType<typeof vi.fn>;
let currentUrl: string;

beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
  hooks.request.mockReset();
  hooks.cancelRestoration.mockReset();
  hooks.clearSelection.mockReset();
  hooks.readSelection.mockReset().mockReturnValue(undefined);
  hooks.navigation.match = undefined;
  hooks.navigation.position = undefined;
  hooks.navigation.intentValue = 1;
  hooks.navigation.openMatch.mockReset().mockResolvedValue(undefined);
  hooks.navigation.selectMatch.mockReset().mockResolvedValue(undefined);
  hooks.navigation.openPosition.mockReset().mockResolvedValue(undefined);
  hooks.navigation.cancel.mockReset();
  hooks.navigation.clear.mockReset();
  hooks.navigation.invalidateMatchAnalysis.mockReset();

  currentUrl = 'http://localhost:8787/';
  replaceState = vi.fn(
    (_state: unknown, _title: string, url?: string | URL) => {
      if (url) currentUrl = String(url);
    },
  );
  scrollTo = vi.fn();
  const storage = new Map<string, string>();
  const makeStorage = () => ({
    getItem: (k: string) => storage.get(k) ?? null,
    setItem: (k: string, v: string) => storage.set(k, v),
    removeItem: (k: string) => storage.delete(k),
  });
  vi.stubGlobal('localStorage', makeStorage());
  vi.stubGlobal('sessionStorage', makeStorage());
  vi.stubGlobal('window', {
    get location() {
      const parsed = new URL(currentUrl);
      return { href: parsed.href, hash: parsed.hash };
    },
    history: { state: null, replaceState },
    scrollTo,
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

function renderNav(
  overrides: Partial<Parameters<typeof useAppNavigation>[0]> = {},
) {
  hooks.index = 0;
  return useAppNavigation({
    initialShowMatchList: false,
    settingsOpen: false,
    focusTab: vi.fn(),
    ...overrides,
  });
}

it('initializes view from hash/selection/matchList, syncs #home hash, and navigates across workspaces', () => {
  const focusTab = vi.fn();
  let nav = renderNav({ focusTab });
  commit();
  expect(nav.view).toBe('home');
  expect(currentUrl).toBe('http://localhost:8787/#home');

  // openMatch with a completed analysis job jumps straight to statistics.
  const job = { id: 'job-1', status: 'completed' } as Job;
  nav.openMatch(sampleMatch, 3, job);
  nav = renderNav({ focusTab });
  commit();
  expect(nav.view).toBe('analysis');
  expect(nav.detail).toBe('statistics');
  expect(nav.requestedAnalysis).toBe(job);
  expect(hooks.navigation.openMatch).toHaveBeenCalledWith('match-1', 3);
  expect(currentUrl).toBe('http://localhost:8787/');
  expect(nav.opened('analysis')).toBe(true);

  // openPausedMatches selects the paused database, switches tab, and moves focus.
  nav.openPausedMatches();
  nav = renderNav({ focusTab });
  expect(nav.view).toBe('database');
  expect(nav.databaseId).toBe(PAUSED_MATCHES_DATABASE_ID);
  expect(focusTab).toHaveBeenCalledWith('database');

  // openCubeDecision switches to analysis errors tab and increments token.
  nav.openCubeDecision({ matchId: 'match-1', gameNo: 2, decisionNo: 7 });
  nav = renderNav({ focusTab });
  expect(nav.view).toBe('analysis');
  expect(nav.detail).toBe('errors');
  expect(nav.requestedCubeDecision).toMatchObject({
    matchId: 'match-1',
    gameNo: 2,
    decisionNo: 7,
    token: 1,
  });

  // goHome ignores modified clicks, handles primary click cleanly.
  const closeImportNotice = vi.fn();
  const modifiedEvent = {
    button: 0,
    metaKey: true,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    preventDefault: vi.fn(),
  } as unknown as MouseEvent<HTMLAnchorElement>;
  nav.goHome(modifiedEvent, closeImportNotice);
  expect(modifiedEvent.preventDefault).not.toHaveBeenCalled();

  const clickEvent = {
    button: 0,
    metaKey: false,
    ctrlKey: false,
    shiftKey: false,
    altKey: false,
    preventDefault: vi.fn(),
  } as unknown as MouseEvent<HTMLAnchorElement>;
  nav.goHome(clickEvent, closeImportNotice);
  nav = renderNav({ focusTab });
  expect(clickEvent.preventDefault).toHaveBeenCalledTimes(1);
  expect(closeImportNotice).toHaveBeenCalledTimes(1);
  expect(scrollTo).toHaveBeenCalledWith(0, 0);
  expect(nav.view).toBe('home');
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

it('fetches move annotation targets and ignores stale responses after navigation intent changes', async () => {
  const moveRes = deferred<{ id: string; game_no: number }>();
  const matchRes = deferred<Match>();
  hooks.request
    .mockReturnValueOnce(moveRes.promise)
    .mockReturnValueOnce(matchRes.promise);

  let nav = renderNav();
  commit();
  nav.openAnnotation({
    target_kind: 'move',
    target_id: 'move-9',
    match_uuid: 'match-1',
    game_no: 4,
  } as Parameters<typeof nav.openAnnotation>[0]);

  expect(hooks.navigation.cancel).toHaveBeenCalledTimes(1);
  // Advance navigation intent before the async load finishes -> stale result ignored.
  hooks.navigation.intentValue = 2;
  moveRes.resolve({ id: 'move-9', game_no: 4 });
  matchRes.resolve(sampleMatch);
  await settle();
  nav = renderNav();
  expect(nav.moveNotes).toBeUndefined();

  // Fresh request matching current intent opens move notes and game.
  hooks.request
    .mockResolvedValueOnce({ id: 'move-10', game_no: 5 })
    .mockResolvedValueOnce(sampleMatch);
  nav.openAnnotation({
    target_kind: 'move',
    target_id: 'move-10',
    match_uuid: 'match-1',
    game_no: 5,
  } as Parameters<typeof nav.openAnnotation>[0]);
  await settle();
  nav = renderNav();
  expect(nav.moveNotes).toEqual({
    match: sampleMatch,
    gameNo: 5,
    moveId: 'move-10',
  });
  expect(hooks.navigation.openMatch).toHaveBeenCalledWith('match-1', 5);
});

it('clears selection when handleLibraryChanged deletes or 404-validates the active match', async () => {
  hooks.navigation.match = sampleMatch;
  hooks.request.mockRejectedValueOnce(
    new AppError('error.request', { status: '404' }),
  );

  let nav = renderNav();
  commit();
  const initialRevision = nav.revision;
  nav.handleLibraryChanged([], true);
  await settle();

  expect(hooks.navigation.clear).toHaveBeenCalledTimes(1);
  expect(hooks.clearSelection).toHaveBeenCalledTimes(1);
  nav = renderNav();
  expect(nav.revision).toBe(initialRevision + 1);

  // Direct deletion in deletedIds clears synchronously without network validation.
  hooks.navigation.clear.mockClear();
  hooks.clearSelection.mockClear();
  nav.handleLibraryChanged(['match-1'], false);
  expect(hooks.navigation.clear).toHaveBeenCalledTimes(1);
  expect(hooks.clearSelection).toHaveBeenCalledTimes(1);
});

it('keeps stable callback references for revision helpers across renders', () => {
  const first = renderNav();
  commit();
  const second = renderNav();
  expect(second.bumpRevision).toBe(first.bumpRevision);
  expect(second.notesSaved).toBe(first.notesSaved);
  expect(second.deckChanged).toBe(first.deckChanged);
  expect(second.bookmarkChanged).toBe(first.bookmarkChanged);
  expect(second.bumpBookRevision).toBe(first.bumpBookRevision);

  const views: AppView[] = ['play', 'analysis'];
  second.selectTab(views[0]);
  expect(renderNav().view).toBe('play');
});
