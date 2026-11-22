import { beforeEach, expect, it, vi } from 'vitest';
import type { Database } from './library-api';

const hooks = vi.hoisted(() => ({
  slots: [] as unknown[],
  index: 0,
  pending: new Map<number, () => (() => void) | void>(),
  request: vi.fn(),
}));
interface EffectSlot {
  deps: unknown[];
  cleanup?: (() => void) | void;
}
vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useState(initial: unknown) {
    const index = hooks.index++;
    if (!(index in hooks.slots)) hooks.slots[index] = initial;
    return [
      hooks.slots[index],
      (next: unknown) => {
        hooks.slots[index] = next;
      },
    ];
  },
  useRef(initial: unknown) {
    return (hooks.slots[hooks.index++] ??= { current: initial });
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
vi.mock('../../shared/api/api', () => ({ request: hooks.request }));
import { useDatabases } from './useDatabases';

const database = (id: string) => ({ id, name: id }) as Database;
function render(...args: Parameters<typeof useDatabases>) {
  hooks.index = 0;
  return useDatabases(...args);
}
function commit() {
  for (const [index, effect] of hooks.pending) {
    const slot = hooks.slots[index] as EffectSlot;
    slot.cleanup?.();
    slot.cleanup = effect();
  }
  hooks.pending.clear();
}
const settle = async () => {
  for (let i = 0; i < 6; i++) await Promise.resolve();
};
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}
beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
  hooks.request.mockReset();
});

it('loads the databases and reports each list to onLoad', async () => {
  const onLoad = vi.fn();
  hooks.request.mockResolvedValueOnce([database('a')]);
  expect(render(0, { onLoad }).databases).toEqual([]);
  commit();
  await settle();
  expect(hooks.request).toHaveBeenCalledWith('/databases', {
    signal: expect.any(AbortSignal),
  });
  expect(render(0, { onLoad })).toEqual({
    databases: [database('a')],
    error: undefined,
  });
  expect(onLoad).toHaveBeenCalledWith([database('a')]);
});

it('waits until enabled and reloads only for a new key', async () => {
  render(0, { enabled: false });
  commit();
  expect(hooks.request).not.toHaveBeenCalled();
  hooks.request.mockResolvedValue([database('a')]);
  render(0, { enabled: true });
  commit();
  await settle();
  render(0, { enabled: true });
  commit();
  expect(hooks.request).toHaveBeenCalledTimes(1);
  render(1, { enabled: true });
  commit();
  expect(hooks.request).toHaveBeenCalledTimes(2);
});

it('ignores a response that a newer load superseded', async () => {
  const first = deferred<Database[]>(),
    second = deferred<Database[]>();
  hooks.request
    .mockReturnValueOnce(first.promise)
    .mockReturnValueOnce(second.promise);
  render(0);
  commit();
  render(1);
  commit();
  expect(
    (hooks.request.mock.calls[0][1] as { signal: AbortSignal }).signal.aborted,
  ).toBe(true);
  second.resolve([database('new')]);
  first.resolve([database('old')]);
  await settle();
  expect(render(1).databases).toEqual([database('new')]);
});

it('keeps a failure until the next successful load', async () => {
  const failure = new Error('offline');
  hooks.request.mockRejectedValueOnce(failure);
  render(0);
  commit();
  await settle();
  expect(render(0).error).toBe(failure);
  const retry = deferred<Database[]>();
  hooks.request.mockReturnValueOnce(retry.promise);
  render(1);
  commit();
  expect(render(1).error).toBe(failure);
  retry.resolve([database('a')]);
  await settle();
  expect(render(1)).toEqual({ databases: [database('a')], error: undefined });
});
