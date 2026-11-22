import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { gamePlanCache, type GamePlans } from './game-plan-api';

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
  useRef(initial: unknown) {
    return (hooks.slots[hooks.index++] ??= { current: initial });
  },
  useCallback(callback: unknown) {
    return callback;
  },
  useEffect(effect: () => (() => void) | void, deps: unknown[]) {
    const index = hooks.index++;
    const previous = hooks.slots[index] as EffectSlot | undefined;
    if (!previous || deps.some((dep, i) => !Object.is(dep, previous.deps[i]))) {
      hooks.slots[index] = { deps, cleanup: previous?.cleanup };
      hooks.pending.set(index, effect);
    }
  },
}));
import { useGamePlans } from './useGamePlans';

const fetch = vi.fn<typeof globalThis.fetch>();
const plans: GamePlans = {
  classifier: 'opensage_rules_v1_opening1',
  source_revision: 'revision',
  game_over: false,
  white: 'anchoring',
  black: 'racing',
};
const board = (point = 24) => {
  const counts = Array<number>(26).fill(0);
  counts[point] = 2;
  counts[1] = -2;
  return counts;
};
function render(counts: number[] | undefined = board(), active = true) {
  hooks.index = 0;
  return useGamePlans(counts, active);
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
  for (let i = 0; i < 18; i++) await Promise.resolve();
}
function deferred() {
  let resolve!: (response: Response) => void;
  const promise = new Promise<Response>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
async function load(counts = board()) {
  fetch.mockResolvedValueOnce(Response.json(plans));
  render(counts);
  commit();
  await settle();
  return render(counts);
}
beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
  gamePlanCache.clear();
  fetch.mockReset();
  vi.stubGlobal('fetch', fetch);
});
afterEach(() => {
  for (const slot of hooks.slots) (slot as EffectSlot | undefined)?.cleanup?.();
  vi.unstubAllGlobals();
});

it('loads only the board and reuses cache across object identities, score, locale and mover', async () => {
  expect((await load()).plans).toEqual(plans);
  render([...board()]);
  commit();
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(JSON.parse(fetch.mock.calls[0][1]!.body as string)).toEqual({
    board: board(),
  });
  expect(
    Object.keys(JSON.parse(fetch.mock.calls[0][1]!.body as string)),
  ).toEqual(['board']);
});

it('hides old labels synchronously while another board is pending', async () => {
  await load();
  fetch.mockReturnValueOnce(deferred().promise);
  expect(render(board(23))).toMatchObject({ loading: true, plans: undefined });
  commit();
  expect(render(board(23)).plans).toBeUndefined();
});

it.each(['A-B', 'A-B-A'] as const)(
  'fences %s responses before cleanup including a return to the same board',
  async (sequence) => {
    const old = deferred(),
      latest = deferred();
    fetch.mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise);
    render(board());
    commit();
    const signal = fetch.mock.calls[0][1]?.signal as AbortSignal;
    render(board(23));
    if (sequence === 'A-B-A') render(board());
    const selected = sequence === 'A-B-A' ? board() : board(23);
    old.resolve(Response.json({ ...plans, white: 'attacking' }));
    await settle();
    expect(render(selected)).toMatchObject({ loading: true, plans: undefined });
    expect(gamePlanCache.get(board())).toBeUndefined();
    commit();
    expect(signal.aborted).toBe(true);
    latest.resolve(Response.json(plans));
    await settle();
    expect(render(selected).plans).toEqual(plans);
  },
);

it('does not fetch while hidden and fences disabling and unmounting', async () => {
  render(board(), false);
  commit();
  expect(fetch).not.toHaveBeenCalled();
  const old = deferred();
  fetch.mockReturnValueOnce(old.promise);
  render(board());
  commit();
  const signal = fetch.mock.calls[0][1]?.signal as AbortSignal;
  expect(render(board(), false)).toMatchObject({
    loading: false,
    plans: undefined,
  });
  old.resolve(Response.json(plans));
  await settle();
  expect(gamePlanCache.get(board())).toBeUndefined();
  commit();
  expect(signal.aborted).toBe(true);
  const unmount = deferred();
  fetch.mockReturnValueOnce(unmount.promise);
  render(board());
  commit();
  for (const slot of hooks.slots) (slot as EffectSlot | undefined)?.cleanup?.();
  unmount.resolve(Response.json(plans));
  await settle();
  expect(gamePlanCache.get(board())).toBeUndefined();
});

it('does not start an old passive effect after a hidden workspace has rendered', () => {
  render(board());
  const precedingEffect = [...hooks.pending.values()][0];
  render(board(), false);
  precedingEffect();
  expect(fetch).not.toHaveBeenCalled();
});

it('reports malformed data and retries explicitly without using a failed result as cache', async () => {
  fetch.mockResolvedValueOnce(Response.json({ ...plans, black: 'optimal' }));
  render();
  commit();
  await settle();
  const failed = render();
  expect(failed.error).toBeDefined();
  expect(failed.loading).toBe(false);
  expect(gamePlanCache.get(board())).toBeUndefined();
  fetch.mockResolvedValueOnce(Response.json(plans));
  failed.retry();
  render();
  commit();
  await settle();
  expect(render().plans).toEqual(plans);
  expect(fetch).toHaveBeenCalledTimes(2);
});

it('uses a cached return immediately while cancelling the intervening board', async () => {
  await load();
  const pending = deferred();
  fetch.mockReturnValueOnce(pending.promise);
  render(board(23));
  commit();
  const signal = fetch.mock.calls[1][1]?.signal as AbortSignal;
  expect(render(board()).plans).toEqual(plans);
  commit();
  expect(signal.aborted).toBe(true);
  pending.resolve(Response.json({ ...plans, white: 'attacking' }));
  await settle();
  expect(render().plans).toEqual(plans);
  expect(fetch).toHaveBeenCalledTimes(2);
});
