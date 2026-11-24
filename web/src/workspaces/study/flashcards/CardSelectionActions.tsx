import ActionButton from '../../../shared/ui/ActionButton';
import { useI18n } from '../../../shared/i18n/i18n';
import { usePositionText } from '../../positions/position-browser-messages';
import type { CardBatchAction } from './DeckLibraryDialogs';
import { useDeckLibraryText } from './deck-library-messages';
import type { Deck } from './learning-api';

/** What can be done with the selected cards. */
export default function CardSelectionActions({
  count,
  decks,
  source,
  ready,
  disabled,
  notice,
  onClear,
  onAction,
}: {
  count: number;
  decks: Deck[];
  /** The listed deck; a collection has none. */
  source?: Deck;
  /** The decks have loaded. */
  ready: boolean;
  disabled: boolean;
  notice?: string;
  onClear: () => void;
  onAction: (mode: CardBatchAction['mode']) => void;
}) {
  const libraryText = useDeckLibraryText();
  const p = usePositionText();
  const { formatNumber } = useI18n();
  const canPickTarget = decks.some((deck) => deck.id !== source?.id);
  const moveLabel = libraryText('move', {
    count: formatNumber(count),
  });
  const moveHint = !ready
    ? undefined
    : decks.length < 2
      ? libraryText('moveNeedsDecks')
      : !source
        ? libraryText('moveNeedsSource')
        : undefined;
  return (
    <>
      <div
        className="position-selection-summary"
        role="group"
        aria-label={libraryText('actions')}
      >
        <strong aria-live="polite">{p('selected', { count: count })}</strong>
        <ActionButton
          icon="clear"
          label={p('clearSelection')}
          compact
          disabled={disabled}
          onClick={onClear}
        />
        <ActionButton
          icon="add"
          label={libraryText('addCards')}
          disabled={disabled || !canPickTarget}
          onClick={() => onAction('add')}
        >
          {libraryText('addCards')}
        </ActionButton>
        <ActionButton
          icon="move"
          label={moveLabel}
          disabled={disabled || !source || !canPickTarget}
          onClick={() => onAction('move')}
        >
          {moveLabel}
        </ActionButton>
        {source && (
          <ActionButton
            icon="delete"
            label={libraryText('removeCards')}
            className="danger"
            disabled={disabled}
            onClick={() => onAction('remove')}
          >
            {libraryText('removeCards')}
          </ActionButton>
        )}
        <ActionButton
          icon="delete"
          label={libraryText('deleteCards')}
          className="danger"
          disabled={disabled}
          onClick={() => onAction('delete')}
        >
          {libraryText('deleteCards')}
        </ActionButton>
        {moveHint && <small>{moveHint}</small>}
      </div>
      {notice && <p role="status">{notice}</p>}
    </>
  );
}
