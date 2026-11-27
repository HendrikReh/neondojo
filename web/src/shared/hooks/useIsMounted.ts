import { useEffect, useRef, type RefObject } from 'react';

/** Tracks whether the current component is mounted, for async handlers. */
export function useIsMounted(): RefObject<boolean> {
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  return mounted;
}
