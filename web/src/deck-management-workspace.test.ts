import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import type { ReactElement } from 'react';
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
  useContext(context: { _currentValue: unknown }) {
    return context._currentValue;
  },
  useRef(initial: unknown) {
    return (hooks.slots[hooks.index++] ??= { current: initial });
  },
  useCallback<T>(callback: T, deps: unknown[]) {
    const index = hooks.index++;
    const previous = hooks.slots[index] as
      { callback: T; deps: unknown[] } | undefined;
    if (previous && deps.every((dep, i) => Object.is(dep, previous.deps[i])))
      return previous.callback;
    hooks.slots[index] = { callback, deps };
    return callback;
  },
  useEffect(effect: () => (() => void) | void, deps: unknown[]) {
    const index = hooks.index++,
      previous = hooks.slots[index] as EffectSlot | undefined;
    if (!previous || deps.some((dep, i) => !Object.is(dep, previous.deps[i]))) {
      hooks.slots[index] = { deps, cleanup: previous?.cleanup };
      hooks.pending.set(index, effect);
    }
  },
}));
import LearningWorkspace, { useDecks } from './LearningPanel';
import ActionButton from './ActionButton';
import { SearchFilterSelect } from './SearchFilterForm';
import StudyCardExercise from './StudyCardExercise';
const decks = [
  { id: 'a', name: 'A', card_count: 1, version: 0 },
  { id: 'b', name: 'B', card_count: 0, version: 0 },
];
const card = {
  id: 'card-a',
  version: 1,
  kind: 'checker',
  players: ['A', 'B'],
  review_count: 0,
  position: { gnubg_match_id: '', player: 0 },
};
const fetch = vi.fn<typeof globalThis.fetch>();
function render(requestedDeck?: { id: string; token: number }, revision = 0) {
  hooks.index = 0;
  return LearningWorkspace({
    theme: 'walnut',
    revision,
    onChanged() {},
    requestedDeck,
  });
}
function elements(node: unknown): ReactElement<Record<string, any>>[] {
  if (!node || typeof node !== 'object') return [];
  if (Array.isArray(node)) return node.flatMap(elements);
  const element = node as ReactElement<Record<string, any>>;
  return [element, ...elements(element.props?.children)];
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
  for (let i = 0; i < 24; i++) await Promise.resolve();
}
async function loaded(requestedDeck?: { id: string; token: number }) {
  render(requestedDeck);
  commit();
  await settle();
  render(requestedDeck);
  commit();
  await settle();
  return render(requestedDeck);
}
beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
  fetch.mockReset();
  // The Anki menu listens for pointer presses outside it.
  vi.stubGlobal('document', {
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  });
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => (key === 'neondojo.deck' ? 'a' : 'en'),
    setItem: vi.fn(),
    removeItem: vi.fn(),
  });
  fetch.mockImplementation(async (path) =>
    Response.json(
      String(path) === '/api/decks'
        ? decks
        : String(path).endsWith('/next')
          ? { card, counts: { due: 1, new: 0 } }
          : { items: [], next_cursor: null },
    ),
  );
  vi.stubGlobal('fetch', fetch);
});
afterEach(() => {
  for (const slot of hooks.slots) (slot as EffectSlot | undefined)?.cleanup?.();
  vi.unstubAllGlobals();
});
it('consumes a missing external requested deck and recovers the selector instead of loading forever', async () => {
  const request = { id: 'removed', token: 1 };
  const view = await loaded(request);
  expect(
    elements(view).some(
      (element) => element.props?.className === 'learning-controls',
    ),
  ).toBe(true);
});
it('hides the old study card synchronously on an A–B–A deck round trip before passive effects', async () => {
  let view = await loaded();
  elements(view)
    .find(
      (element) =>
        element.type === ActionButton && element.props.label === 'Study now',
    )!
    .props.onClick();
  render();
  commit();
  await settle();
  view = render();
  expect(
    elements(view).some((element) => element.type === StudyCardExercise),
  ).toBe(true);
  elements(view)
    .find((element) => element.type === SearchFilterSelect)!
    .props.onChange('b');
  view = render();
  elements(view)
    .find((element) => element.type === SearchFilterSelect)!
    .props.onChange('a');
  view = render();
  elements(view)
    .find(
      (element) =>
        element.type === ActionButton && element.props.label === 'Study now',
    )!
    .props.onClick();
  view = render();
  expect(
    elements(view).some((element) => element.type === StudyCardExercise),
  ).toBe(false);
});

it('a late deck-list read cannot resurrect a deck removed locally or overwrite a rename', async () => {
  let resolve!: (response: Response) => void;
  fetch.mockReturnValueOnce(
    new Promise<Response>((done) => {
      resolve = done;
    }),
  );
  hooks.index = 0;
  let state = useDecks(0);
  commit();
  state.addDeck({ ...decks[0], created_at: '', due_count: 1 });
  hooks.index = 0;
  state = useDecks(0);
  state.replaceDeck({
    ...decks[0],
    name: 'Renamed',
    version: 1,
    created_at: '',
    due_count: 1,
  });
  state.removeDeck('a');
  resolve(Response.json(decks));
  await settle();
  hooks.index = 0;
  state = useDecks(0);
  expect(state.decks).toEqual([]);
});

it('clears an in-flight export indicator when switching decks', async () => {
  let view = await loaded();
  fetch.mockReturnValueOnce(new Promise<Response>(() => {}));
  elements(view)
    .find(
      (element) =>
        element.type === ActionButton &&
        element.props.label === 'Download Anki deck',
    )!
    .props.onClick();
  view = render();
  elements(view)
    .find((element) => element.type === SearchFilterSelect)!
    .props.onChange('b');
  render();
  commit();
  view = render();
  expect(
    elements(view).some(
      (element) =>
        element.type === ActionButton &&
        element.props.label === 'Download Anki deck',
    ),
  ).toBe(true);
});

it('waits for the current revision before consuming an external deck missing from an older ready list', async () => {
  await loaded();
  let resolve!: (response: Response) => void;
  fetch.mockReturnValueOnce(
    new Promise<Response>((done) => {
      resolve = done;
    }),
  );
  render(undefined, 1);
  commit();
  await settle();
  const requested = { id: 'new-deck', token: 1 };
  render(requested, 1);
  commit();
  await settle();
  let view = render(requested, 1);
  expect(
    elements(view).some(
      (element) => element.props?.className === 'learning-controls',
    ),
  ).toBe(false);
  resolve(
    Response.json([
      ...decks,
      { id: requested.id, name: 'New deck', card_count: 1, version: 1 },
    ]),
  );
  await settle();
  render(requested, 1);
  commit();
  render(requested, 1);
  commit();
  await settle();
  view = render(requested, 1);
  expect(
    elements(view).find((element) => element.type === SearchFilterSelect)!.props
      .value,
  ).toBe('new-deck');
  expect(
    elements(view).some((element) => element.type === StudyCardExercise),
  ).toBe(true);
  expect(
    fetch.mock.calls.some(
      ([path]) => String(path) === '/api/decks/new-deck/next',
    ),
  ).toBe(true);
});

it('consumes a requested deck that is absent after the current revision actually settles', async () => {
  await loaded();
  let resolve!: (response: Response) => void;
  fetch.mockReturnValueOnce(
    new Promise<Response>((done) => {
      resolve = done;
    }),
  );
  const requested = { id: 'deleted', token: 2 };
  render(requested, 1);
  commit();
  await settle();
  expect(
    elements(render(requested, 1)).some(
      (element) => element.props?.className === 'learning-controls',
    ),
  ).toBe(false);
  resolve(Response.json(decks));
  await settle();
  render(requested, 1);
  commit();
  await settle();
  const view = render(requested, 1);
  expect(
    elements(view).some(
      (element) => element.props?.className === 'learning-controls',
    ),
  ).toBe(true);
  expect(
    fetch.mock.calls.some(
      ([path]) => String(path) === '/api/decks/deleted/next',
    ),
  ).toBe(false);
});

it.each([1, 2])(
  'a rename preserves the current page only for its own single revision (delta %s)',
  async (delta) => {
    fetch.mockImplementation(async (path) =>
      Response.json(
        String(path) === '/api/decks'
          ? decks
          : {
              items: [
                {
                  ...card,
                  id: String(path).includes('page=2')
                    ? 'later-card'
                    : 'first-card',
                },
              ],
              total: 51,
              next_cursor: null,
            },
      ),
    );
    let view = await loaded();
    elements(view)
      .find((element) => element.props?.onPageChange)!
      .props.onPageChange(2);
    render();
    commit();
    await settle();
    view = render();
    expect(
      elements(view).find((element) => element.props?.cards)!.props.cards,
    ).toMatchObject([{ id: 'later-card' }]);
    elements(view)
      .find(
        (element) =>
          element.type === ActionButton &&
          element.props.label === 'Manage decks',
      )!
      .props.onClick();
    view = render();
    elements(view)
      .find((element) => element.props?.manage)!
      .props.manage('rename', decks[0]);
    view = render();
    const readsBefore = fetch.mock.calls.filter(([path]) =>
      String(path).includes('/cards?'),
    ).length;
    elements(view)
      .find((element) => element.props?.onRenamed)!
      .props.onRenamed({ ...decks[0], name: 'Renamed', version: 1 });
    render(undefined, delta);
    commit();
    await settle();
    view = render(undefined, delta);
    const readsAfter = fetch.mock.calls.filter(([path]) =>
      String(path).includes('/cards?'),
    ).length;
    expect(readsAfter - readsBefore).toBe(delta === 1 ? 0 : 1);
    expect(
      elements(view).find((element) => element.props?.cards)!.props.cards,
    ).toMatchObject([{ id: delta === 1 ? 'later-card' : 'first-card' }]);
  },
);
