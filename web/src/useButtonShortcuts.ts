import { useEffect } from 'react';

const TEXT_ENTRY =
  'input,textarea,select,[contenteditable="true"],[contenteditable=""]';
/** Elements that already act on Enter themselves. */
const OWN_ENTER = `${TEXT_ENTRY},button,a,summary,[role="tab"],[role="row"],[role="option"],[role="separator"]`;

/** "D", "1", "Enter" or "Shift+E"; undefined for keys that are never shortcuts. */
export function shortcutToken(event: {
  key: string;
  shiftKey: boolean;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
}): string | undefined {
  if (event.altKey || event.ctrlKey || event.metaKey) return undefined;
  const key =
    event.key === 'Enter'
      ? 'Enter'
      : /^[a-z0-9]$/i.test(event.key)
        ? event.key.toUpperCase()
        : undefined;
  if (!key) return undefined;
  return event.shiftKey ? `Shift+${key}` : key;
}

/**
 * Press the visible, enabled button that declares `data-shortcut` for the key.
 * An open dialog confines the search, so a practice dialog never triggers a
 * control of the workspace behind it.
 */
export function useButtonShortcuts() {
  useEffect(() => {
    function press(event: KeyboardEvent) {
      if (event.defaultPrevented || event.repeat) return;
      const token = shortcutToken(event);
      if (!token) return;
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest(token === 'Enter' ? OWN_ENTER : TEXT_ENTRY)) return;
      const dialogs = document.querySelectorAll('dialog[open]');
      const scope: ParentNode = dialogs[dialogs.length - 1] ?? document;
      const button = Array.from(
        scope.querySelectorAll<HTMLButtonElement>('button[data-shortcut]'),
      ).find(
        (candidate) =>
          candidate.dataset.shortcut === token &&
          !candidate.disabled &&
          candidate.getAttribute('aria-disabled') !== 'true' &&
          candidate.getClientRects().length > 0,
      );
      if (!button) return;
      event.preventDefault();
      button.click();
    }
    window.addEventListener('keydown', press);
    return () => window.removeEventListener('keydown', press);
  }, []);
}
