import type { ReactNode } from 'react';
import type { Theme } from '../../../shared/board/themes';
import ActionButton from '../../../shared/ui/ActionButton';
import AnkiActionsMenu from './AnkiActionsMenu';
import AnkiAutoSync from './AnkiAutoSync';
import CardPracticeActions, {
  type SpecialPractice,
} from './CardPracticeActions';
import CardSearchControls from './CardSearchControls';
import CardSelectionActions from './CardSelectionActions';
import CardTagEditor from './CardTagEditor';
import CardTopics from './CardTopics';
import type { CardBatchAction } from './DeckLibraryDialogs';
import LearningCardBrowser from './LearningCardBrowser';
import TransferPractice from './TransferPractice';
import { useCardPracticeText } from './card-practice-messages';
import {
  emptyCardFilters,
  normalizeCardFilters,
  type CardFilters,
} from './card-search-api';
import { useCardSearchText } from './card-search-messages';
import type { VersionedDeck } from './deck-management-api';
import type { CardPage, PracticeRound } from './learning-api';
import { useLearningText } from './learning-messages';
import type { AnkiTransfers } from './useAnkiTransfers';

export default function CardBrowseView({
  visible,
  theme,
  deckId,
  decks,
  ready,
  sourceDeck,
  deckSelector,
  filters,
  draftFilters,
  listKey,
  cards,
  total,
  loading,
  page,
  selectedCards,
  selectedItems,
  cardNotice,
  studyDisabled,
  anki,
  practice,
  onDraftFiltersChange,
  onApplyFilters,
  onClearFilters,
  onSelectTopic,
  onStartStudy,
  onAutoSyncReceived,
  onStartTransferPractice,
  onToggleCards,
  onClearSelection,
  onOpenCardAction,
  onOpenSpecialPractice,
  onPageChange,
  onChanged,
}: {
  visible: boolean;
  theme: Theme;
  deckId: string;
  decks: VersionedDeck[];
  ready: boolean;
  sourceDeck?: VersionedDeck;
  deckSelector: ReactNode;
  filters: CardFilters;
  draftFilters: CardFilters;
  listKey: string;
  cards: CardPage['items'];
  total: number;
  loading: boolean;
  page: number;
  selectedCards: ReadonlySet<string>;
  selectedItems: readonly CardPage['items'][number][];
  cardNotice?: string;
  studyDisabled: boolean;
  anki: AnkiTransfers;
  practice: {
    limit: number;
    starting: boolean;
    error: unknown;
    resumable: boolean;
    onLimit: (limit: number) => void;
    onStart: () => void;
    onResume: () => void;
  };
  onDraftFiltersChange: (filters: CardFilters) => void;
  onApplyFilters: (filters: CardFilters) => void;
  onClearFilters: (filters: CardFilters) => void;
  onSelectTopic: (deck: string, filters: CardFilters) => void;
  onStartStudy: () => void;
  onAutoSyncReceived: () => void;
  onStartTransferPractice: (round: PracticeRound) => void;
  onToggleCards: (ids: string[], checked: boolean) => void;
  onClearSelection: () => void;
  onOpenCardAction: (mode: CardBatchAction['mode']) => void;
  onOpenSpecialPractice: (
    practice: SpecialPractice,
    trigger: HTMLButtonElement,
  ) => void;
  onPageChange: (page: number) => void;
  onChanged: () => void;
}) {
  const t = useLearningText();
  const searchT = useCardSearchText();
  const practiceT = useCardPracticeText();
  const actionDisabled = !selectedCards.size || loading;
  const selectedItem =
    selectedItems.length === 1 ? selectedItems[0] : undefined;

  return (
    <>
      <CardSearchControls
        filters={draftFilters}
        onChange={onDraftFiltersChange}
        onSearch={() => onApplyFilters(normalizeCardFilters(draftFilters))}
        onClear={() => onClearFilters(emptyCardFilters())}
      >
        {deckSelector}
      </CardSearchControls>
      {sourceDeck && (
        <div className="learning-deck-panel position-deck-panel">
          <h2>{sourceDeck.name}</h2>
          <div className="learning-controls">
            <ActionButton
              icon="play"
              label={t('studyNow')}
              className="primary"
              disabled={studyDisabled}
              onClick={onStartStudy}
            >
              {t('studyNow')}
            </ActionButton>
            <AnkiActionsMenu
              deck={sourceDeck}
              exporting={anki.exporting}
              busy={anki.busy}
              onExport={() => void anki.exportDeck(sourceDeck)}
              onSend={() => void anki.send(sourceDeck)}
              onSync={() => void anki.sync(sourceDeck)}
              onImport={(files) => void anki.importPackages(files)}
            />
          </div>
          {visible && (
            <AnkiAutoSync
              key={sourceDeck.id}
              deckId={sourceDeck.id}
              empty={!sourceDeck.card_count}
              busy={anki.busy}
              onReceived={onAutoSyncReceived}
            />
          )}
          <p className="learning-explainer">{searchT('scopeHint')}</p>
        </div>
      )}
      {/* Secondary study tools stay folded so the cards follow the search. */}
      <details className="learning-extras">
        <summary>{practiceT('studyTools')}</summary>
        <TransferPractice onStart={onStartTransferPractice} />
        <CardTopics
          filters={filters}
          deck={deckId}
          onSelect={onSelectTopic}
        />
      </details>
      <LearningCardBrowser
        key={listKey}
        visible={visible}
        cards={cards}
        total={total}
        loading={loading}
        decks={decks}
        selection={{
          ids: selectedCards,
          toggle: onToggleCards,
          disabled: loading,
        }}
        actions={
          <>
            <CardPracticeActions
              cards={cards}
              loading={loading}
              limit={practice.limit}
              onLimit={practice.onLimit}
              starting={practice.starting}
              error={practice.error}
              resumable={practice.resumable}
              selectedItem={selectedItem}
              onStart={practice.onStart}
              onResume={practice.onResume}
              onSpecial={onOpenSpecialPractice}
            >
              <CardSelectionActions
                count={selectedCards.size}
                decks={decks}
                source={sourceDeck}
                ready={ready}
                disabled={actionDisabled}
                notice={cardNotice}
                onClear={onClearSelection}
                onAction={onOpenCardAction}
              />
            </CardPracticeActions>
            <CardTagEditor
              key={[...selectedCards].sort().join(',')}
              cards={selectedItems.map((item) => ({
                id: item.id,
                tags_version: item.browse.tags_version ?? 0,
              }))}
              tags={selectedItem?.browse.card_tags}
              onSaved={onChanged}
            />
          </>
        }
        theme={theme}
        page={page}
        onPageChange={onPageChange}
      />
    </>
  );
}
