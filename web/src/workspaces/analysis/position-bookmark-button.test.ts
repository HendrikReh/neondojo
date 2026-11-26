import { isValidElement, type ReactElement, type ReactNode } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { PositionBookmark } from '../positions/position-bookmark-api';

// Match the existing Node hook tests while retaining real JSX and API requests.
// Effects are committed separately to exercise navigation before old cleanup.
const hooks = vi.hoisted(() => ({
  slots: [] as unknown[],
  index: 0,
  pending: new Map<number, () => (() => void) | void>(),
}));
interface EffectSlot {
  deps: unknown[];
  cleanup?: (() => void) | void;
}
vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  // A render of this harness commits at once.
  useInsertionEffect(effect: () => void) {
    effect();
  },
  useContext(context: { _currentValue: unknown }) {
    return context._currentValue;
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
    const index = hooks.index++;
    return (hooks.slots[index] ??= { current: initial });
  },
  useCallback(callback: unknown) {
    return callback;
  },
  useMemo(factory: () => unknown) {
    return factory();
  },
  useLayoutEffect() {},
  useEffect(effect: () => (() => void) | void, deps: unknown[]) {
    const index = hooks.index++;
    const previous = hooks.slots[index] as EffectSlot | undefined;
    if (!previous || deps.some((dep, i) => !Object.is(dep, previous.deps[i]))) {
      hooks.slots[index] = { deps, cleanup: previous?.cleanup };
      hooks.pending.set(index, effect);
    }
  },
}));
vi.mock('../../shared/i18n/i18n', () => ({
  useI18n: () => ({
    locale: 'en',
    t: (key: string) => key,
    formatNumber: String,
    formatDecision: String,
    formatError: () => 'Request failed',
  }),
}));
import PositionBookmarkButton from '../positions/PositionBookmarkButton';
import ActionButton from '../../shared/ui/ActionButton';
import { positionBookmarkMessages } from '../positions/position-bookmark-messages';
import AnalysisWorkspace from './AnalysisWorkspace';
import AnalysisPanel from './AnalysisPanel';
import BoardReviewControls from './BoardReviewControls';
import type { Position } from '../../shared/http/api';

type Props = Parameters<typeof PositionBookmarkButton>[0];
type Element = ReactElement<Record<string, unknown>>;
const changed = vi.fn();
const fetch = vi.fn<typeof globalThis.fetch>();
const defaults = { positionId: 'source-one', revision: 0, onChanged: changed };
function render(props: Partial<Props> = {}) {
  hooks.index = 0;
  return PositionBookmarkButton({ ...defaults, ...props });
}
function elements(node: ReactNode): Element[] {
  if (Array.isArray(node)) return node.flatMap(elements);
  if (!isValidElement<Record<string, unknown>>(node)) return [];
  return [node, ...elements(node.props.children as ReactNode)];
}
function text(node: ReactNode): string {
  if (Array.isArray(node)) return node.map(text).join('');
  if (isValidElement<{ children?: ReactNode; label?: string }>(node))
    return node.type === ActionButton
      ? (node.props.label ?? '')
      : text(node.props.children);
  return typeof node === 'string' || typeof node === 'number'
    ? String(node)
    : '';
}
function button(tree: ReactNode, label: string) {
  const result = elements(tree).find(
    (node) =>
      (node.type === 'button' || node.type === ActionButton) &&
      text(node) === label,
  );
  expect(result, label).toBeDefined();
  return result!;
}
function click(tree: ReactNode, label: string) {
  const target = button(tree, label);
  expect(target.props.disabled).not.toBe(true);
  (target.props.onClick as () => void)();
}
function commit() {
  for (const [index, effect] of hooks.pending) {
    const slot = hooks.slots[index] as EffectSlot;
    slot.cleanup?.();
    slot.cleanup = effect();
  }
  hooks.pending.clear();
}
async function settle() {
  for (let i = 0; i < 16; i++) await Promise.resolve();
}
function status(id: string, saved: boolean): PositionBookmark {
  return {
    position_id: id,
    saved,
    saved_at: saved ? '2026-11-09T10:00:00Z' : null,
  };
}
function deferred() {
  let resolve!: (response: Response) => void;
  const promise = new Promise<Response>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
async function load(saved = false) {
  fetch.mockResolvedValueOnce(Response.json(status('source-one', saved)));
  render();
  commit();
  await settle();
  return render();
}

beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
  changed.mockReset();
  fetch.mockReset();
  vi.stubGlobal('fetch', fetch);
});
afterEach(() => {
  for (const slot of hooks.slots) (slot as EffectSlot | undefined)?.cleanup?.();
  vi.unstubAllGlobals();
});

it('loads neutral collection status without automatically saving', async () => {
  const pending = deferred();
  fetch.mockReturnValueOnce(pending.promise);
  let tree = render();
  expect(text(tree)).not.toContain('Save position');
  commit();
  pending.resolve(Response.json(status('source-one', false)));
  await settle();
  tree = render();
  expect(button(tree, 'Save position').props['aria-pressed']).toBe(false);
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(fetch.mock.calls[0][1]?.method).toBeUndefined();
  expect(changed).not.toHaveBeenCalled();
});

it('saves and removes only after explicit actions with confirmed pressed state', async () => {
  let tree = await load();
  const saving = deferred();
  fetch.mockReturnValueOnce(saving.promise);
  click(tree, 'Save position');
  tree = render();
  expect(button(tree, 'Saving…').props.disabled).toBe(true);
  expect(button(tree, 'Saving…').props['aria-pressed']).toBe(false);
  saving.resolve(Response.json(status('source-one', true)));
  await settle();
  tree = render();
  expect(button(tree, 'Remove from collection').props['aria-pressed']).toBe(
    true,
  );
  expect(changed).toHaveBeenCalledTimes(1);
  fetch.mockResolvedValueOnce(new Response(null, { status: 204 }));
  click(tree, 'Remove from collection');
  await settle();
  tree = render();
  expect(button(tree, 'Save position').props['aria-pressed']).toBe(false);
  expect(changed).toHaveBeenCalledTimes(2);
  expect(
    fetch.mock.calls.map(([url, init]) => [url, init?.method ?? 'GET']),
  ).toEqual([
    ['/api/positions/source-one/bookmark', 'GET'],
    ['/api/positions/source-one/bookmark', 'PUT'],
    ['/api/positions/source-one/bookmark', 'DELETE'],
  ]);
});

it('retries a failed initial load without presenting unknown membership as unsaved', async () => {
  fetch.mockResolvedValueOnce(
    Response.json({ error: 'Unavailable' }, { status: 503 }),
  );
  render();
  commit();
  await settle();
  let tree = render();
  expect(text(tree)).not.toContain('Save position');
  expect(text(tree)).toContain('Could not load collection status.');
  fetch.mockResolvedValueOnce(Response.json(status('source-one', true)));
  click(tree, 'Retry');
  render();
  commit();
  await settle();
  tree = render();
  expect(button(tree, 'Remove from collection').props['aria-pressed']).toBe(
    true,
  );
  expect(changed).not.toHaveBeenCalled();
});

it('retains confirmed membership and retries the same failed removal', async () => {
  let tree = await load(true);
  fetch.mockResolvedValueOnce(
    Response.json({ error: 'Unavailable' }, { status: 503 }),
  );
  click(tree, 'Remove from collection');
  await settle();
  tree = render();
  expect(button(tree, 'Remove from collection').props['aria-pressed']).toBe(
    true,
  );
  expect(text(tree)).toContain('Could not remove position.');
  expect(changed).not.toHaveBeenCalled();
  fetch.mockResolvedValueOnce(new Response(null, { status: 204 }));
  click(tree, 'Retry');
  await settle();
  expect(button(render(), 'Save position').props['aria-pressed']).toBe(false);
  expect(fetch.mock.calls[2][1]?.method).toBe('DELETE');
});

it.each(['GET', 'PUT', 'DELETE'] as const)(
  'ignores a stale %s response as soon as a new source position renders',
  async (method) => {
    const pending = deferred();
    if (method === 'GET') {
      fetch.mockReturnValueOnce(pending.promise);
      render();
      commit();
    } else {
      const tree = await load(method === 'DELETE');
      fetch.mockReturnValueOnce(pending.promise);
      click(
        tree,
        method === 'PUT' ? 'Save position' : 'Remove from collection',
      );
    }
    let tree = render({ positionId: 'source-two' });
    expect(text(tree)).not.toContain('Save position');
    pending.resolve(
      method === 'DELETE'
        ? new Response(null, { status: 204 })
        : Response.json(status('source-one', true)),
    );
    await settle();
    tree = render({ positionId: 'source-two' });
    expect(text(tree)).not.toContain('Remove from collection');
    expect(changed).not.toHaveBeenCalled();
    fetch.mockResolvedValueOnce(Response.json(status('source-two', false)));
    commit();
    await settle();
    expect(
      button(render({ positionId: 'source-two' }), 'Save position').props[
        'aria-pressed'
      ],
    ).toBe(false);
  },
);

it('refetches after collection changes but not when the callback identity changes', async () => {
  await load(true);
  render({ onChanged: () => {} });
  commit();
  expect(fetch).toHaveBeenCalledTimes(1);
  fetch.mockResolvedValueOnce(Response.json(status('source-one', false)));
  render({ revision: 1 });
  commit();
  await settle();
  expect(
    button(render({ revision: 1 }), 'Save position').props['aria-pressed'],
  ).toBe(false);
  expect(fetch).toHaveBeenCalledTimes(2);
});

it('provides complete explicit collection actions in both languages', () => {
  expect(Object.keys(positionBookmarkMessages)).toHaveLength(2);
  for (const catalog of Object.values(positionBookmarkMessages)) {
    expect(Object.keys(catalog)).toEqual(
      Object.keys(positionBookmarkMessages.en),
    );
    for (const message of Object.values(catalog))
      expect(message.trim()).not.toBe('');
  }
});

it('uses the real source occurrence beside review controls during candidate and final previews', () => {
  const board = Array<number>(26).fill(0);
  board[13] = 1;
  const position: Position = {
    id: 'source-occurrence',
    match_uuid: 'match',
    ordinal: 1,
    game_no: 1,
    ply: 1,
    player: 0,
    dice: [6, 1],
    board,
    position_id: 'board-id',
    gnubg_match_id: '',
    notation: '13/6',
  };
  const game = {
    game_no: 1,
    position_count: 1,
    white_score: 0,
    black_score: 0,
    crawford: false,
  };
  const noop = () => {};
  const props = {
    navigation: {
      match: {
        id: 'match',
        source_available: true,
        white: 'Ada',
        black: 'Grace',
        filename: 'a.mat',
        format: 'mat',
        match_length: 7,
        position_count: 1,
        created_at: '',
      },
      game,
      games: [game],
      gameNo: 1,
      positions: [position],
      position,
      index: 0,
      loading: false,
      error: null,
      gameLoaded: true,
      selectIndex: noop,
      previous: noop,
      next: noop,
      cancel: noop,
      clearError: noop,
      clear: noop,
      invalidateMatchAnalysis: noop,
      intent: () => 0,
      selectMatch: async () => false,
      selectGame: async () => false,
      openPosition: async () => false,
      openMatch: async () => false,
      resume: async () => {},
    },
    jobs: {
      jobs: {},
      refresh: () => {},
      removeDice: async () => {},
      ...{ completedAnalysis: undefined },
      error: null,
      submitting: false,
      restoring: false,
      failure: undefined,
      run: async () => {},
    },
    theme: 'forest',
    defaultPlies: 2,
    openingBookRevision: 0,
    configured: true,
    detail: 'overview',
    onDetail: noop,
    onOpenDatabase: noop,
    onTrackJob: noop,
    onNotesSaved: noop,
    onMoveNotes: noop,
    notesSuspended: false,
    deckRevision: 0,
    onDeckChanged: noop,
    onPosition: noop,
    welcome: null,
    bookmarkRevision: 3,
    onBookmarkChanged: changed,
  } satisfies Parameters<typeof AnalysisWorkspace>[0];
  function workspace() {
    hooks.index = 0;
    return AnalysisWorkspace(props);
  }
  let tree = workspace();
  const panel = elements(tree).find((node) => node.type === AnalysisPanel)!;
  (panel.props.onPreview as (candidate: unknown) => void)({
    rank: 2,
    notation: '13/7 7/6',
    source: 'GNUbg',
  });
  tree = workspace();
  const controls = elements(tree).find(
    (node) => node.type === BoardReviewControls,
  )!;
  (controls.props.onView as (view: string) => void)('after');
  tree = workspace();
  const toolbar = elements(tree).find(
    (node) => node.props.className === 'board-review-toolbar',
  );
  expect(toolbar).toBeDefined();
  const bookmark = elements(toolbar).find(
    (node) => node.type === PositionBookmarkButton,
  );
  expect(bookmark?.props).toMatchObject({
    positionId: 'source-occurrence',
    revision: 3,
    onChanged: changed,
  });
  props.navigation.position = undefined as unknown as Position;
  props.navigation.positions = [];
  expect(
    elements(workspace()).some((node) => node.type === PositionBookmarkButton),
  ).toBe(false);
});
