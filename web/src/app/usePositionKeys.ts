import { useEffect } from 'react';
import type { useGameNavigation } from './useGameNavigation';

/** The arrow keys step through the positions outside of controls. */
export function usePositionKeys(
  navigation: ReturnType<typeof useGameNavigation>,
  enabled: boolean,
) {
  useEffect(() => {
    function move(event: globalThis.KeyboardEvent) {
      if (!enabled || event.altKey || event.ctrlKey || event.metaKey) return;
      const target = event.target as HTMLElement | null;
      if (
        target?.closest(
          'input,textarea,select,button,[contenteditable="true"],[role="tablist"],[role="dialog"]',
        )
      )
        return;
      if (event.key === 'ArrowLeft') {
        event.preventDefault();
        navigation.previous();
      }
      if (event.key === 'ArrowRight') {
        event.preventDefault();
        navigation.next();
      }
    }
    window.addEventListener('keydown', move);
    return () => window.removeEventListener('keydown', move);
  }, [enabled, navigation]);
}
