import { afterEach, beforeEach, expect, it, vi } from 'vitest';

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
  useEffect(effect: () => (() => void) | void, deps: unknown[]) {
    const index = hooks.index++;
    const prior = hooks.slots[index] as EffectSlot | undefined;
    if (!prior || deps.some((value, i) => !Object.is(value, prior.deps[i]))) {
      hooks.slots[index] = { deps, cleanup: prior?.cleanup };
      hooks.pending.set(index, effect);
    }
  },
}));

import {
  useVersionedResource,
  type VersionedResource,
} from './useVersionedResource';

interface Item extends VersionedResource {
  name: string;
}

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

function render(
  id: string | undefined,
  load: (id: string, signal: AbortSignal) => Promise<Item>,
  revision = 0,
) {
  hooks.index = 0;
  return useVersionedResource(id, load, revision);
}

beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
});

afterEach(() => {
  for (const slot of hooks.slots) {
    if (slot && typeof slot === 'object' && 'cleanup' in slot) {
      (slot as EffectSlot).cleanup?.();
    }
  }
});

it('does nothing when id is undefined and hides stale details across id changes', async () => {
  const signals: AbortSignal[] = [];
  const load = vi.fn(async (id: string, signal: AbortSignal) => {
    signals.push(signal);
    return { id, version: 1, name: `item-${id}` };
  });

  expect(render(undefined, load)).toMatchObject({
    details: undefined,
    error: undefined,
  });
  commit();
  expect(load).not.toHaveBeenCalled();

  render('a', load);
  commit();
  await settle();
  expect(render('a', load).details).toEqual({
    id: 'a',
    version: 1,
    name: 'item-a',
  });

  // Before commit/cleanup, switching id immediately hides previous item's details.
  expect(render('b', load).details).toBeUndefined();
  commit();
  expect(signals[0].aborted).toBe(true);
  await settle();
  expect(render('b', load).details).toEqual({
    id: 'b',
    version: 1,
    name: 'item-b',
  });
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

it('ignores out-of-order responses with a lower version for the same id', async () => {
  const first = deferred<Item>();
  const load = vi.fn(() => first.promise);

  render('a', load, 0);
  commit();

  // Optimistic / newer version written locally before the older load settles.
  render('a', load, 0).setDetails({ id: 'a', version: 5, name: 'newest' });
  first.resolve({ id: 'a', version: 3, name: 'stale' });
  await settle();

  expect(render('a', load, 0).details).toEqual({
    id: 'a',
    version: 5,
    name: 'newest',
  });

  // Equal or higher versions replace stored state.
  const newer = deferred<Item>();
  load.mockReturnValueOnce(newer.promise);
  render('a', load, 1);
  commit();
  newer.resolve({ id: 'a', version: 6, name: 'fresh' });
  await settle();
  expect(render('a', load, 1).details).toEqual({
    id: 'a',
    version: 6,
    name: 'fresh',
  });
});

it('records load errors per id and recovers on retry', async () => {
  const failure = new Error('network failed');
  const load = vi
    .fn<(id: string, signal: AbortSignal) => Promise<Item>>()
    .mockRejectedValueOnce(failure)
    .mockResolvedValueOnce({ id: 'a', version: 2, name: 'recovered' });

  render('a', load);
  commit();
  await settle();

  const failed = render('a', load);
  expect(failed.error).toBe(failure);
  expect(failed.details).toBeUndefined();
  // Switching to another id does not leak the previous id's error.
  expect(render('b', load).error).toBeUndefined();

  // Retry triggers a fresh load and clears error.
  render('a', load).retry();
  render('a', load);
  commit();
  await settle();
  const recovered = render('a', load);
  expect(recovered.error).toBeUndefined();
  expect(recovered.details).toEqual({ id: 'a', version: 2, name: 'recovered' });
});
