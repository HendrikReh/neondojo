import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type SetStateAction,
} from 'react';

/** Keep an applied value in this browser tab, seeding it once from old settings. */
export function useWorkspaceSession<T>(
  name: string,
  seed: T | undefined,
  fallback: T,
  decode: (stored: unknown) => T | undefined,
  scope: 'session' | 'layout' = 'session',
) {
  const key = `bgtutor.${scope}.${name}.v1`;
  const storage = () => (scope === 'layout' ? localStorage : sessionStorage);
  const [stored, setStored] = useState<T | undefined>(() => {
    try {
      const raw = storage().getItem(key);
      return raw === null ? undefined : decode(JSON.parse(raw));
    } catch {
      return undefined;
    }
  });
  const latest = useRef(stored);
  const initial = seed === undefined ? fallback : seed;
  const value = stored === undefined ? initial : stored;
  const save = useCallback(
    (next: T) => {
      latest.current = next;
      setStored(next);
      try {
        (scope === 'layout' ? localStorage : sessionStorage).setItem(
          key,
          JSON.stringify(next),
        );
      } catch {
        // The workspace still works when browser storage is unavailable.
      }
    },
    [key, scope],
  );

  useEffect(() => {
    if (latest.current === undefined && seed !== undefined) save(seed);
  }, [save, seed]);

  return [
    value,
    (next: SetStateAction<T>) =>
      save(
        typeof next === 'function'
          ? (next as (previous: T) => T)(
              latest.current === undefined ? value : latest.current,
            )
          : next,
      ),
  ] as const;
}
