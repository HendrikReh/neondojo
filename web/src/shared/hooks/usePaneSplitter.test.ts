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
  useLayoutEffect(effect: () => (() => void) | void, deps: unknown[]) {
    const index = hooks.index++;
    const prior = hooks.slots[index] as EffectSlot | undefined;
    if (!prior || deps.some((value, i) => !Object.is(value, prior.deps[i]))) {
      hooks.slots[index] = { deps, cleanup: prior?.cleanup };
      hooks.pending.set(index, effect);
    }
  },
}));

import { usePaneSplitter } from './usePaneSplitter';

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

function fakeContainer() {
  const properties = new Map<string, string>();
  const element = {
    style: {
      setProperty: (key: string, value: string) => properties.set(key, value),
    },
  } as unknown as HTMLElement;
  return { ref: { current: element }, properties };
}

function fakeSeparator() {
  const attrs = new Map<string, string>();
  let captured: number | null = null;
  const focus = vi.fn();
  const element = {
    focus,
    setAttribute: (key: string, value: string) => attrs.set(key, value),
    setPointerCapture: (id: number) => {
      captured = id;
    },
    hasPointerCapture: (id: number) => captured === id,
    releasePointerCapture: (id: number) => {
      if (captured === id) captured = null;
    },
  };
  return {
    element: element as unknown as HTMLElement,
    attrs,
    focus,
    captured: () => captured,
  };
}

let frames: Map<number, FrameRequestCallback>;
let nextFrameId: number;

beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
  frames = new Map();
  nextFrameId = 1;
  vi.stubGlobal('requestAnimationFrame', (cb: FrameRequestCallback) => {
    const id = nextFrameId++;
    frames.set(id, cb);
    return id;
  });
  vi.stubGlobal('cancelAnimationFrame', (id: number) => {
    frames.delete(id);
  });
});

afterEach(() => {
  unmount();
  vi.unstubAllGlobals();
});

function flushFrames() {
  const pending = [...frames.entries()];
  frames.clear();
  for (const [, cb] of pending) cb(16);
}

function renderSplitter(
  overrides: Partial<Parameters<typeof usePaneSplitter>[0]> = {},
) {
  hooks.index = 0;
  const container = fakeContainer();
  const onCommit = vi.fn();
  const onReset = vi.fn();
  const result = usePaneSplitter({
    containerRef: container.ref,
    cssVar: '--pane-width',
    width: 300,
    min: 200,
    max: 500,
    onCommit,
    onReset,
    ariaLabel: 'Resize pane',
    ariaControls: 'pane-1',
    helpText: 'Drag to resize',
    className: 'resizer',
    ...overrides,
  });
  return { ...result, container, onCommit, onReset };
}

it('drags with clamping, updates CSS var via rAF, and commits final width on pointerup', () => {
  const container = fakeContainer();
  const onCommit = vi.fn();
  const onReset = vi.fn();
  const sep = fakeSeparator();
  const base = {
    containerRef: container.ref,
    cssVar: '--pane-width',
    width: 300,
    min: 200,
    max: 500,
    onCommit,
    onReset,
  };

  let state = renderSplitter(base);
  commit();
  const props = state.handle.props as Record<
    string,
    (...args: unknown[]) => void
  >;

  // Ignores non-primary button.
  props.onPointerDown({
    button: 2,
    isPrimary: true,
    pointerId: 1,
    clientX: 100,
    currentTarget: sep.element,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  });
  expect(sep.captured()).toBeNull();

  // Primary pointer starts drag and captures pointer.
  props.onPointerDown({
    button: 0,
    isPrimary: true,
    pointerId: 7,
    clientX: 100,
    currentTarget: sep.element,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  });
  expect(sep.captured()).toBe(7);
  expect(sep.focus).toHaveBeenCalledTimes(1);

  state = renderSplitter(base);
  expect(state.dragging).toBe(true);

  // Move beyond max clamps to 500 and coalesces to one rAF.
  const moveProps = state.handle.props as Record<
    string,
    (...args: unknown[]) => void
  >;
  moveProps.onPointerMove({ pointerId: 7, clientX: 600 });
  moveProps.onPointerMove({ pointerId: 7, clientX: 350 });
  expect(frames.size).toBe(1);
  flushFrames();
  expect(container.properties.get('--pane-width')).toBe('500px');
  expect(sep.attrs.get('aria-valuenow')).toBe('500');

  // Move back inside bounds and release.
  moveProps.onPointerMove({ pointerId: 7, clientX: 160 });
  moveProps.onPointerUp({ pointerId: 7 });
  expect(sep.captured()).toBeNull();
  expect(container.properties.get('--pane-width')).toBe('360px');
  expect(onCommit).toHaveBeenCalledWith(360);
  expect(renderSplitter(base).dragging).toBe(false);
});

it('cancels an active drag on Escape or resetKey change and restores startWidth without committing', () => {
  const container = fakeContainer();
  const onCommit = vi.fn();
  const onReset = vi.fn();
  const sep = fakeSeparator();
  const base = {
    containerRef: container.ref,
    cssVar: '--pane-width',
    width: 320,
    min: 200,
    max: 500,
    direction: 'left' as const,
    resetKey: 'tab-a',
    onCommit,
    onReset,
  };

  let state = renderSplitter(base);
  commit();
  let props = state.handle.props as Record<
    string,
    (...args: unknown[]) => void
  >;
  props.onPointerDown({
    button: 0,
    isPrimary: true,
    pointerId: 3,
    clientX: 400,
    currentTarget: sep.element,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  });
  // Left direction grows width when moving pointer left (negative deltaX).
  props.onPointerMove({ pointerId: 3, clientX: 340 });
  flushFrames();
  expect(container.properties.get('--pane-width')).toBe('380px');

  // Escape cancels and restores 320px.
  props.onKeyDown({
    key: 'Escape',
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  });
  expect(container.properties.get('--pane-width')).toBe('320px');
  expect(sep.captured()).toBeNull();
  expect(onCommit).not.toHaveBeenCalled();

  // Start another drag and cancel via resetKey change.
  state = renderSplitter(base);
  props = state.handle.props as Record<string, (...args: unknown[]) => void>;
  props.onPointerDown({
    button: 0,
    isPrimary: true,
    pointerId: 4,
    clientX: 400,
    currentTarget: sep.element,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  });
  props.onPointerMove({ pointerId: 4, clientX: 350 });
  renderSplitter({ ...base, resetKey: 'tab-b' });
  commit();
  expect(container.properties.get('--pane-width')).toBe('320px');
  expect(sep.captured()).toBeNull();
  expect(onCommit).not.toHaveBeenCalled();
});

it('supports keyboard stepping, Shift coarse stepping, Home/End, and Enter/double-click reset', () => {
  const onCommit = vi.fn();
  const onReset = vi.fn();
  const state = renderSplitter({
    width: 300,
    min: 200,
    max: 500,
    direction: 'left',
    onCommit,
    onReset,
  });
  commit();
  const props = state.handle.props as Record<
    string,
    (...args: unknown[]) => void
  >;
  const key = (keyName: string, shiftKey = false) =>
    props.onKeyDown({
      key: keyName,
      shiftKey,
      preventDefault: vi.fn(),
      stopPropagation: vi.fn(),
    });

  // direction='left' reverses ArrowLeft (+step) and ArrowRight (-step).
  key('ArrowLeft');
  key('ArrowRight', true);
  key('Home');
  key('End');
  key('Enter');
  props.onDoubleClick();

  expect(onCommit.mock.calls).toEqual([[310], [250], [200], [500]]);
  expect(onReset).toHaveBeenCalledTimes(2);
});
