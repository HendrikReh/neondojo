import { useEffect } from 'react';
import type { BoardView } from '../board/move-preview';

export function useBoardReviewShortcuts(
  enabled: boolean,
  reviewButtonSelector: string,
  setView: (view: BoardView) => void,
  canShowAfter = true,
) {
  useEffect(() => {
    if (!enabled) return;
    function changeView(event: KeyboardEvent) {
      if (
        event.defaultPrevented ||
        (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') ||
        event.altKey ||
        event.ctrlKey ||
        event.metaKey ||
        event.shiftKey
      )
        return;
      const target = event.target instanceof Element ? event.target : null;
      if (
        document.querySelector('dialog[open]') ||
        target?.closest(
          'input,textarea,select,a,[contenteditable="true"],[contenteditable=""],[role="tablist"],[role="dialog"],[role="separator"]',
        )
      )
        return;
      const button = target?.closest('button');
      if (
        button &&
        !button.matches(reviewButtonSelector) &&
        !button.closest('.board-view-buttons')
      )
        return;

      event.preventDefault();
      event.stopPropagation();
      if (event.key === 'ArrowLeft') setView('before');
      else if (canShowAfter) setView('after');
    }
    // Handle the active review before the app's match-history arrow shortcuts.
    window.addEventListener('keydown', changeView, true);
    return () => window.removeEventListener('keydown', changeView, true);
  }, [enabled, reviewButtonSelector, setView, canShowAfter]);
}
