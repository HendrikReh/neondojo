import { useRef, useState, type Dispatch, type SetStateAction } from 'react';
import { useOptionalConfiguration } from './ConfigurationProvider';
import type { ProfileConfiguration } from './configuration-model';

/** A controlled profile value with a legacy fallback for isolated workspaces. */
export function useProfileValue<T>(
  select: (configuration: ProfileConfiguration) => T,
  replace: (
    configuration: ProfileConfiguration,
    value: T,
  ) => ProfileConfiguration,
  initial: T | (() => T),
): [T, Dispatch<SetStateAction<T>>] {
  const profile = useOptionalConfiguration();
  const [fallback, setFallback] = useState(initial);
  const selected = profile?.current ? select(profile.current) : fallback;
  const key = JSON.stringify(selected, (_key, value: unknown) =>
    value && typeof value === 'object' && !Array.isArray(value)
      ? Object.fromEntries(
          Object.keys(value)
            .sort()
            .map((key) => [key, (value as Record<string, unknown>)[key]]),
        )
      : value,
  );
  // Canonical writes may clone an unrelated section. Query effects depend on
  // semantic changes, rather than those fresh object identities.
  const stable = useRef({ key, value: selected });
  if (stable.current.key !== key) stable.current = { key, value: selected };
  const value = stable.current.value;
  const setValue: Dispatch<SetStateAction<T>> = (next) => {
    if (profile) {
      profile.updateCurrent((configuration) =>
        replace(
          configuration,
          typeof next === 'function'
            ? (next as (previous: T) => T)(select(configuration))
            : next,
        ),
      );
    } else setFallback(next);
  };
  return [value, setValue];
}
