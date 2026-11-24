import {
  useCallback,
  useInsertionEffect,
  useRef,
  useState,
  type RefObject,
} from 'react';

/**
 * A ref to the value of the last committed render, for event handlers, effects
 * and promises. It changes before any effect of the same commit runs, and a
 * render that React discards does not touch it. Do not read it while rendering.
 */
export function useLatest<T>(value: T): RefObject<T> {
  const latest = useRef(value);
  useInsertionEffect(() => {
    latest.current = value;
  });
  return latest;
}

/**
 * A function that never changes and calls the latest one, so that a memoized
 * child does not render again for it. Call it from events, not while rendering.
 */
export function useLatestCallback<A extends unknown[], R>(
  callback: (...args: A) => R,
): (...args: A) => R {
  const latest = useLatest(callback);
  return useCallback((...args: A) => latest.current(...args), [latest]);
}

/**
 * A number that grows with every change of the key, so A, B, A are three
 * epochs. Compare it with a `useLatest` of it to ignore late responses.
 */
export function useEpoch(key: string | undefined): number {
  const [state, setState] = useState({ key, epoch: 0 });
  if (state.key === key) return state.epoch;
  const next = { key, epoch: state.epoch + 1 };
  setState(next);
  return next.epoch;
}
