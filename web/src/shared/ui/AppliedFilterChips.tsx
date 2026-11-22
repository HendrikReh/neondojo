import ActionIcon from './action-icons';
import { usePositionText } from '../../position-browser-messages';
import './applied-filter-chips.css';

export type AppliedFilterChip<Key extends string> = {
  key: Key;
  text: string;
  removeLabel?: string;
};

export default function AppliedFilterChips<Key extends string>({
  chips,
  disabled,
  onRemove,
}: {
  chips: readonly AppliedFilterChip<Key>[];
  disabled?: boolean;
  onRemove: (key: Key, button: HTMLButtonElement) => void;
}) {
  const t = usePositionText();
  if (!chips.length) return null;
  return (
    <ul className="applied-filter-chips" aria-label={t('appliedFilters')}>
      {chips.map(({ key, text, removeLabel }) => (
        <li key={key}>
          <button
            type="button"
            aria-label={removeLabel ?? t('removeFilter', { filter: text })}
            title={text}
            disabled={disabled}
            onClick={(event) => onRemove(key, event.currentTarget)}
          >
            <span>{text}</span>
            <ActionIcon name="close" />
          </button>
        </li>
      ))}
    </ul>
  );
}
