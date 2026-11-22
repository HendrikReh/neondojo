import {
  SearchFilterInput,
  SearchFilterSelect,
} from '../../../shared/ui/SearchFilterForm';
import { ALL_CARDS, CARDS_WITHOUT_DECK } from './card-search-api';
import { useCardSearchText } from './card-search-messages';
import { groupDecks } from './deck-groups';
import type { VersionedDeck } from './deck-management-api';
import { useLearningText } from './learning-messages';

/** The deck choice in Study, grouped by deck path and filterable for many decks. */
export default function DeckSelector({
  decks,
  deckId,
  onChoose,
  filter,
  onFilter,
}: {
  decks: VersionedDeck[];
  deckId: string;
  onChoose: (id: string) => void;
  filter: string;
  onFilter: (filter: string) => void;
}) {
  const t = useLearningText();
  const searchT = useCardSearchText();
  const deckOption = (deck: (typeof decks)[number], label: string) => (
    <option value={deck.id} key={deck.id}>
      {label} · {t('cards', { count: deck.card_count })}
    </option>
  );
  return (
    <>
      {decks.length > 12 && (
        <SearchFilterInput
          label={t('filterDecks')}
          type="search"
          value={filter}
          onChange={onFilter}
        />
      )}
      <SearchFilterSelect
        label={t('chooseDeck')}
        value={deckId}
        onChange={onChoose}
      >
        <option value={ALL_CARDS}>{searchT('allCards')}</option>
        <option value={CARDS_WITHOUT_DECK}>{searchT('withoutDeck')}</option>
        {groupDecks(decks, filter, deckId).map((group) =>
          group.path ? (
            <optgroup key={group.path} label={group.path}>
              {group.decks.map(({ deck, label }) => deckOption(deck, label))}
            </optgroup>
          ) : (
            group.decks.map(({ deck, label }) => deckOption(deck, label))
          ),
        )}
      </SearchFilterSelect>
    </>
  );
}
