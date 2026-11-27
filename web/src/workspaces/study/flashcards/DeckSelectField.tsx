import { useId } from 'react';
import type { Deck } from './learning-api';

/** Shared deck `<select>` field with optional placeholder and card-count hint. */
export default function DeckSelectField({
  decks,
  value,
  onChange,
  label,
  disabled = false,
  placeholder,
  countText,
}: {
  decks: readonly Pick<Deck, 'id' | 'name' | 'card_count'>[];
  value: string;
  onChange: (deckId: string) => void;
  label: string;
  disabled?: boolean;
  /** Disabled placeholder option shown when no deck is selected. */
  placeholder?: string;
  /** Optional card-count hint linked via `aria-describedby`. */
  countText?: string;
}) {
  const countId = useId();
  return (
    <label>
      <span>{label}</span>
      <select
        aria-label={label}
        aria-describedby={countText ? countId : undefined}
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
      >
        {placeholder ? (
          !value && (
            <option value="" disabled>
              {placeholder}
            </option>
          )
        ) : (
          !decks.length && <option value="">—</option>
        )}
        {decks.map((deck) => (
          <option key={deck.id} value={deck.id}>
            {deck.name}
          </option>
        ))}
      </select>
      {countText && <small id={countId}>{countText}</small>}
    </label>
  );
}
