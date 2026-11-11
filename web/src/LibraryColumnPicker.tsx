import { useI18n } from './i18n';
import ActionButton from './ActionButton';
import { useEffect, useRef } from 'react';
import { useLibraryText } from './library-messages';
import type { LibraryDataColumn } from './library-column-preferences';

export default function LibraryColumnPicker({
  options,
  shown,
  onToggle,
  onAll,
  onReset,
  onMove,
}: {
  options: readonly { id: LibraryDataColumn; label: string }[];
  shown: readonly LibraryDataColumn[];
  onToggle: (id: LibraryDataColumn, checked: boolean) => void;
  onAll: () => void;
  onReset: () => void;
  onMove?: (id: LibraryDataColumn, delta: -1 | 1) => void;
}) {
  const { locale } = useI18n();
  const labels = {
    de: ['Nach oben', 'Nach unten'],
    en: ['Move up', 'Move down'],
    es: ['Mover arriba', 'Mover abajo'],
    fr: ['Monter', 'Descendre'],
    it: ['Sposta su', 'Sposta giù'],
    pl: ['Przenieś w górę', 'Przenieś w dół'],
  }[locale];
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
          {options.map((option, index) => (
            <div className="library-column-option" key={option.id}>
              <label>
                <input
                  type="checkbox"
                  checked={shown.includes(option.id)}
                  disabled={shown.length === 1 && shown.includes(option.id)}
                  onChange={(event) =>
                    onToggle(option.id, event.target.checked)
                  }
                />
                <span>{option.label}</span>
              </label>
              {onMove && (
                <span className="library-column-order-controls">
                  <ActionButton
                    icon="sort-up"
                    compact
                    label={`${labels[0]}: ${option.label}`}
                    type="button"
                    disabled={index === 0}
                    onClick={() => onMove(option.id, -1)}
                  />
                  <ActionButton
                    icon="sort-down"
                    compact
                    label={`${labels[1]}: ${option.label}`}
                    type="button"
                    disabled={index === options.length - 1}
                    onClick={() => onMove(option.id, 1)}
                  />
                </span>
              )}
            </div>
          ))}
        </div>
        <small>{l('columnSelectionHint')}</small>
        <div className="library-column-menu-actions">
          <ActionButton
            icon="select-all"
            label={l('showAllColumns')}
            type="button"
            onClick={onAll}
          />
          <ActionButton
            icon="reset"
            label={l('defaultColumns')}
            type="button"
            onClick={onReset}
          />
        </div>
      </div>
    </details>
  );
}
