import { useEffect, useRef } from 'react';
import { useLibraryText } from './library-messages';
import type { LibraryDataColumn } from './library-column-preferences';

export default function LibraryColumnPicker({
  options,
  shown,
  onToggle,
  onAll,
  onReset,
}: {
  options: readonly { id: LibraryDataColumn; label: string }[];
  shown: readonly LibraryDataColumn[];
  onToggle: (id: LibraryDataColumn, checked: boolean) => void;
  onAll: () => void;
  onReset: () => void;
}) {
  const l = useLibraryText(),
    root = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const close = (event: PointerEvent) => {
      if (
        root.current?.open &&
        event.target instanceof Node &&
        !root.current.contains(event.target)
      )
        root.current.open = false;
    };
    document.addEventListener('pointerdown', close);
    return () => document.removeEventListener('pointerdown', close);
  }, []);
  return (
    <details
      className="library-column-picker"
      ref={root}
      onKeyDown={(event) => {
        if (event.key === 'Escape' && root.current?.open) {
          event.preventDefault();
          root.current.open = false;
          root.current.querySelector('summary')?.focus();
        }
      }}
    >
      <summary>{l('columns')}</summary>
      <div
        className="library-column-menu"
        role="group"
        aria-label={l('chooseColumns')}
      >
        <strong>{l('chooseColumns')}</strong>
        <div className="library-column-options">
          {options.map((option) => (
            <label key={option.id}>
              <input
                type="checkbox"
                checked={shown.includes(option.id)}
                disabled={shown.length === 1 && shown.includes(option.id)}
                onChange={(event) => onToggle(option.id, event.target.checked)}
              />
              <span>{option.label}</span>
            </label>
          ))}
        </div>
        <small>{l('columnSelectionHint')}</small>
        <div className="library-column-menu-actions">
          <button type="button" onClick={onAll}>
            {l('showAllColumns')}
          </button>
          <button type="button" onClick={onReset}>
            {l('defaultColumns')}
          </button>
        </div>
      </div>
    </details>
  );
}
