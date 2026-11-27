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

import { usePolling } from './usePolling';

function commit() {
  for (const [index, effect] of hooks.pending) {
    const slot = hooks.slots[index] as EffectSlot;
    slot.cleanup?.();
    slot.cleanup = effect();
  }
  hooks.pending.clear();
}

function unmount() {
  for (const slot of hooks.slots) {
    if (slot && typeof slot === 'object' && 'cleanup' in slot) {
      (slot as EffectSlot).cleanup?.();
      (slot as EffectSlot).cleanup = undefined;
    }
  }
  hooks.pending.clear();
}

function render(
  poll: Parameters<typeof usePolling>[0],
  options: Parameters<typeof usePolling>[1],
) {
  hooks.index = 0;
  usePolling(poll, options);
}

beforeEach(() => {
  vi.useFakeTimers();
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
});

afterEach(() => {
  unmount();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

it('polls immediately when active, reschedules only on true, and stops when inactive', async () => {
  const signals: AbortSignal[] = [];
  const poll = vi.fn(async (signal: AbortSignal) => {
    signals.push(signal);
    return signals.length < 3;
  });

  render(poll, { active: false, intervalMs: 1000 });
  commit();
  await vi.advanceTimersByTimeAsync(5000);
  expect(poll).not.toHaveBeenCalled();

  render(poll, { active: true, intervalMs: 1000 });
  commit();
  await vi.advanceTimersByTimeAsync(0);
  expect(poll).toHaveBeenCalledTimes(1);

  await vi.advanceTimersByTimeAsync(1000);
  expect(poll).toHaveBeenCalledTimes(2);

  await vi.advanceTimersByTimeAsync(1000);
  expect(poll).toHaveBeenCalledTimes(3);

  // Returning false stops further scheduling.
  await vi.advanceTimersByTimeAsync(10_000);
  expect(poll).toHaveBeenCalledTimes(3);
  expect(signals.every((signal) => !signal.aborted)).toBe(true);
});

it('uses the latest poll callback without restarting the timer and aborts on cleanup or dep change', async () => {
  const seen: string[] = [];
  const signals: AbortSignal[] = [];
  const firstPoll = vi.fn(async (signal: AbortSignal) => {
    signals.push(signal);
    seen.push('first');
    return true;
  });
  const secondPoll = vi.fn(async (signal: AbortSignal) => {
    signals.push(signal);
    seen.push('second');
    return true;
  });

  render(firstPoll, { active: true, intervalMs: 2000, deps: ['a'] });
  commit();
  await vi.advanceTimersByTimeAsync(0);
  expect(seen).toEqual(['first']);

  // Updating callback identity alone does not abort or restart early.
  render(secondPoll, { active: true, intervalMs: 2000, deps: ['a'] });
  commit();
  expect(signals[0].aborted).toBe(false);
  await vi.advanceTimersByTimeAsync(2000);
  expect(seen).toEqual(['first', 'second']);

  // Changing deps aborts the previous session and restarts immediately.
  render(secondPoll, { active: true, intervalMs: 2000, deps: ['b'] });
  commit();
  expect(signals[0].aborted).toBe(true);
  await vi.advanceTimersByTimeAsync(0);
  expect(seen).toEqual(['first', 'second', 'second']);

  unmount();
  expect(signals.at(-1)?.aborted).toBe(true);
});

it('refreshes on window focus and visibilitychange only when visible and idle', async () => {
  const windowListeners = new Map<string, Set<() => void>>();
  const docListeners = new Map<string, Set<() => void>>();
  let visibilityState: DocumentVisibilityState = 'visible';

  vi.stubGlobal('window', {
    addEventListener: (type: string, fn: () => void) => {
      (
        windowListeners.get(type) ??
        windowListeners.set(type, new Set()).get(type)!
      ).add(fn);
    },
    removeEventListener: (type: string, fn: () => void) => {
      windowListeners.get(type)?.delete(fn);
    },
  });
  vi.stubGlobal('document', {
    get visibilityState() {
      return visibilityState;
    },
    addEventListener: (type: string, fn: () => void) => {
      (
        docListeners.get(type) ?? docListeners.set(type, new Set()).get(type)!
      ).add(fn);
    },
    removeEventListener: (type: string, fn: () => void) => {
      docListeners.get(type)?.delete(fn);
    },
  });

  let finish!: (again: boolean) => void;
  const poll = vi.fn(
    () =>
      new Promise<boolean>((resolve) => {
        finish = resolve;
      }),
  );

  render(poll, { active: true, intervalMs: 1000, refreshOnVisible: true });
  commit();
  expect(poll).toHaveBeenCalledTimes(1);

  // Ignored while already reading.
  for (const fn of windowListeners.get('focus') ?? []) fn();
  expect(poll).toHaveBeenCalledTimes(1);

  // Settle without scheduling a timer.
  finish(false);
  await vi.advanceTimersByTimeAsync(0);

  // Hidden visibilitychange is ignored.
  visibilityState = 'hidden';
  for (const fn of docListeners.get('visibilitychange') ?? []) fn();
  expect(poll).toHaveBeenCalledTimes(1);

  // Visible focus triggers an immediate poll.
  visibilityState = 'visible';
  for (const fn of windowListeners.get('focus') ?? []) fn();
  expect(poll).toHaveBeenCalledTimes(2);

  // When a timer is already queued, focus does not double-poll.
  finish(true);
  await vi.advanceTimersByTimeAsync(0);
  for (const fn of docListeners.get('visibilitychange') ?? []) fn();
  expect(poll).toHaveBeenCalledTimes(2);

  unmount();
  expect(windowListeners.get('focus')?.size ?? 0).toBe(0);
  expect(docListeners.get('visibilitychange')?.size ?? 0).toBe(0);
});
