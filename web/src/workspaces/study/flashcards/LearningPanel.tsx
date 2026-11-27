import { useCallback, useEffect, useRef, useState } from 'react';
import { useWorkspaceDefaults } from '../../../settings/configuration/ConfigurationProvider';
import {
  sessionFields,
  sessionString,
  useWorkspaceSession,
} from '../../../shared/hooks/useWorkspaceSession';
import { useSearchPages } from '../../../shared/hooks/useSearchPages';
import { request } from '../../../shared/http/api';
import { useI18n } from '../../../shared/i18n/i18n';
import { dateTimeFormat } from '../../../shared/i18n/i18n-core';
import type { Theme } from '../../../shared/board/themes';
import ActionButton from '../../../shared/ui/ActionButton';
import { useStudyOverviewText } from '../study-messages';
import AnkiTransferStatus from './AnkiTransferStatus';
import CardBrowseView from './CardBrowseView';
import type { SpecialPractice } from './CardPracticeActions';
import CardPracticeRound from './CardPracticeRound';
import CardReviewView from './CardReviewView';
import type { CardBatchAction } from './DeckLibraryDialogs';
import DeckSelector from './DeckSelector';
import LearningDialogs, {
  type DeckManagementRequest,
  type LibraryDialogKind,
} from './LearningDialogs';
import LearningHeader from './LearningHeader';
import {
  ALL_CARDS,
  CARD_PAGE_SIZE,
  CARD_SELECTION_LIMIT,
  cardListPath,
  cardPracticePath,
  emptyCardFilters,
  isCollectionScope,
} from './card-search-api';
import type { CardPage } from './learning-api';
import { useLearningText } from './learning-messages';
import { useAnkiTransfers } from './useAnkiTransfers';
import { useCardPractice } from './useCardPractice';
import { savedDeck, useDecks } from './useDecks';
import { useFsrsReviewSession } from './useFsrsReviewSession';
import './learning-workspace.css';

const defaultCardFilters = emptyCardFilters();

export default function LearningWorkspace({
  visible = true,
  theme,
  revision,
  onChanged,
  requestedDeck,
}: {
  visible?: boolean;
  theme: Theme;
  revision: number;
  onChanged: () => void;
  requestedDeck?: { id: string; token: number };
}) {
  const legacy = useWorkspaceDefaults();
  const t = useLearningText();
  const overviewText = useStudyOverviewText();
  const { t: uiT, locale, formatError } = useI18n();
  const [practiceLimit, setPracticeLimit] = useState(10);
  const [specialPractice, setSpecialPractice] = useState<SpecialPractice>();
  const specialPracticeTrigger = useRef<HTMLButtonElement | null>(null);
  const externalAnkiInput = useRef<HTMLInputElement>(null);
  const [editedFilters, setDraftFilters] =
    useState<ReturnType<typeof emptyCardFilters>>();
  const [searchRun, setSearchRun] = useState(0);
  const [historyRevision, setHistoryRevision] = useState(0);
  const [filters, setFilters] = useWorkspaceSession(
    'study.filters',
    legacy?.learning.filters,
    defaultCardFilters,
    (value) =>
      sessionFields(value, {
        ...emptyCardFilters(),
        topics: '',
        game_plan: '',
      }),
  );
  const draftFilters = editedFilters ?? filters;
  const [libraryDialog, setLibraryDialog] = useState<LibraryDialogKind>();
  const [cardAction, setCardAction] = useState<CardBatchAction>();
  const [cardNotice, setCardNotice] = useState<string>();
  const [cardSelection, setCardSelection] = useState<{
    scope: string;
    items: Map<string, CardPage['items'][number]>;
  }>();
  const [management, setManagement] = useState<DeckManagementRequest | null>(
    null,
  );
  const {
    decks,
    error: deckError,
    addDeck,
    replaceDeck,
    removeDeck,
    refresh,
    ready,
    settled,
  } = useDecks(revision);
  const [deckFilter, setDeckFilter] = useState('');
  const [deckId, setDeckId] = useWorkspaceSession(
    'study.deck',
    legacy?.learning.deck_id,
    savedDeck(),
    sessionString,
  );
  // A rename changes the revision but not the listed cards.
  const [metadataOnlyRevision, setMetadataOnlyRevision] = useState<number>();
  const [list, setList] = useState({ revision, value: 0 });
  let contentRevision = list.value;
  if (list.revision !== revision) {
    if (revision !== metadataOnlyRevision) contentRevision++;
    setList({ revision, value: contentRevision });
  }
  const searchKey = JSON.stringify([deckId, filters, searchRun]);
  const listKey = JSON.stringify([searchKey, contentRevision]);
  const {
    round: practiceRound,
    setRound: setPracticeRound,
    resumeRound,
    rememberRound,
    starting: practiceStarting,
    error: practiceError,
    open: openPractice,
  } = useCardPractice({
    scope: listKey,
    visible,
    startPath: () => cardPracticePath(deckId, filters, practiceLimit),
  });
  const review = useFsrsReviewSession({
    deckId,
    decks,
    settled,
    requestedDeck,
    onSelectDeck: setDeckId,
    onClearPractice: () => setPracticeRound(undefined),
    onChanged,
  });

  function chooseDeck(id: string) {
    setCardNotice(undefined);
    setDeckId(id);
    review.leaveStudyForDeckChange();
  }

  const selectedItems =
    cardSelection?.scope === listKey ? [...cardSelection.items.values()] : [];
  const selectedCards = new Set(selectedItems.map((item) => item.id));
  const sourceDeck = decks.find((deck) => deck.id === deckId);

  function toggleCards(ids: string[], checked: boolean) {
    setCardSelection((previous) => {
      const next = new Map(previous?.scope === listKey ? previous.items : []);
      for (const item of cardSearch.items) {
        if (!ids.includes(item.id)) continue;
        if (!checked) next.delete(item.id);
        else if (next.size < CARD_SELECTION_LIMIT) next.set(item.id, item);
      }
      return { scope: listKey, items: next };
    });
  }

  const loadCards = useCallback(
    (signal: AbortSignal, page: number) =>
      request<CardPage>(cardListPath(deckId, filters, page), { signal }),
    [deckId, filters],
  );
  const cardSearch = useSearchPages({
    scope: listKey,
    revision: historyRevision,
    pageSize: CARD_PAGE_SIZE,
    enabled: !!deckId,
    loadPage: loadCards,
  });
  const anki = useAnkiTransfers({
    deckId,
    onImported: () => {
      cardSearch.invalidateResults();
      onChanged();
    },
    onReviewsSynced: () => {
      setHistoryRevision((value) => value + 1);
      onChanged();
    },
  });

  useEffect(() => {
    if (
      settled &&
      !review.studying &&
      !isCollectionScope(deckId) &&
      !decks.some((d) => d.id === deckId)
    )
      setDeckId(ALL_CARDS);
  }, [decks, deckId, settled, review.studying]);

  useEffect(() => {
    if (deckId) {
      review.setError(null);
      anki.clearExportError();
    }
  }, [listKey]);

  function openCardAction(mode: CardBatchAction['mode']) {
    if (!selectedCards.size || cardSearch.loading) return;
    const ids = [...selectedCards];
    setCardNotice(undefined);
    if (mode === 'delete') {
      const cards = selectedItems.map((item) => ({
        id: item.id,
        version: item.version,
        deck_ids: item.browse.decks.map((deck) => deck.id),
      }));
      if (cards.length === ids.length) setCardAction({ mode, cards });
    } else if (mode === 'add') {
      setCardAction({ mode, ids, source: sourceDeck });
    } else if (sourceDeck) {
      setCardAction({ mode, ids, source: sourceDeck });
    }
  }

  function refreshCardActions() {
    setCardAction(undefined);
    setCardSelection(undefined);
    cardSearch.invalidateResults();
    onChanged();
  }

  if (review.waitingForRequestedDeck)
    return (
      <section className="learning-workspace" aria-label={t('study')}>
        <h1>{t('study')}</h1>
        <p role="status">{t('loading')}</p>
        {!!deckError && (
          <>
            <p role="alert">{formatError(deckError)}</p>
            <ActionButton
              icon="refresh"
              label={uiT('games.retry')}
              compact
              onClick={onChanged}
            >
              {uiT('games.retry')}
            </ActionButton>
          </>
        )}
      </section>
    );

  const deckSelector = (
    <DeckSelector
      decks={decks}
      deckId={deckId}
      onChoose={chooseDeck}
      filter={deckFilter}
      onFilter={setDeckFilter}
    />
  );
  const workspaceError =
    review.error || anki.exportError || cardSearch.error || deckError;
  // Reviewing a deck gives the page to the card: the heading and the library
  // actions return with the card search.
  const reviewing = review.studying && !practiceRound;

  return (
    <section className="learning-workspace" aria-label={t('study')}>
      <LearningHeader
        reviewing={reviewing}
        studying={review.studying}
        inPracticeRound={!!practiceRound}
        activeNext={review.activeNext}
        deckSelector={deckSelector}
        ankiBusy={anki.busy}
        externalAnkiInput={externalAnkiInput}
        onStopStudy={review.stopStudy}
        onOpenLibraryDialog={setLibraryDialog}
        onImportExternal={(files) => void anki.importPackages(files, true)}
      />
      <AnkiTransferStatus transfers={anki} />
      {!!workspaceError && <p role="alert">{formatError(workspaceError)}</p>}
      {review.missingRequestedDeck && (
        <p role="alert">{overviewText('deckUnavailable')}</p>
      )}
      {review.reviewResult && (
        <p className="learning-review-result" role="status">
          {t('nextDue', {
            date: dateTimeFormat(locale, {
              dateStyle: 'medium',
              timeStyle: 'short',
            }).format(new Date(review.reviewResult.due_at)),
          })}
        </p>
      )}
      {practiceRound ? (
        <CardPracticeRound
          key={practiceRound.id}
          round={practiceRound}
          theme={theme}
          visible={visible}
          onChange={setPracticeRound}
          onClose={() => {
            if (
              practiceRound.index >= practiceRound.total &&
              !practiceRound.summary.retry_count
            )
              rememberRound(null);
            setPracticeRound(undefined);
          }}
        />
      ) : review.studying ? (
        <CardReviewView
          deckId={deckId}
          deckName={sourceDeck?.name}
          theme={theme}
          visible={visible}
          review={review}
        />
      ) : (
        <CardBrowseView
          visible={visible}
          theme={theme}
          deckId={deckId}
          decks={decks}
          ready={ready}
          sourceDeck={sourceDeck}
          deckSelector={deckSelector}
          filters={filters}
          draftFilters={draftFilters}
          listKey={listKey}
          cards={cardSearch.items}
          total={cardSearch.total ?? cardSearch.items.length}
          loading={cardSearch.loading}
          page={cardSearch.page}
          selectedCards={selectedCards}
          selectedItems={selectedItems}
          cardNotice={cardNotice}
          studyDisabled={review.loading || review.busy || review.attemptBusy}
          anki={anki}
          practice={{
            limit: practiceLimit,
            starting: practiceStarting,
            error: practiceError,
            resumable: !!resumeRound,
            onLimit: setPracticeLimit,
            onStart: () => void openPractice(),
            onResume: () => {
              if (resumeRound) void openPractice(resumeRound);
            },
          }}
          onDraftFiltersChange={setDraftFilters}
          onApplyFilters={(nextFilters) => {
            setCardNotice(undefined);
            setFilters(nextFilters);
            setSearchRun((value) => value + 1);
          }}
          onClearFilters={(empty) => {
            setCardNotice(undefined);
            setDraftFilters(empty);
            setFilters(empty);
            setSearchRun((value) => value + 1);
          }}
          onSelectTopic={(deck, nextFilters) => {
            chooseDeck(deck);
            setDraftFilters(nextFilters);
            setFilters(nextFilters);
            setSearchRun((value) => value + 1);
          }}
          onStartStudy={review.startStudy}
          onAutoSyncReceived={() => setHistoryRevision((value) => value + 1)}
          onStartTransferPractice={(round) => {
            rememberRound(round.id);
            setPracticeRound(round);
          }}
          onToggleCards={toggleCards}
          onClearSelection={() => setCardSelection(undefined)}
          onOpenCardAction={openCardAction}
          onOpenSpecialPractice={(practice, trigger) => {
            specialPracticeTrigger.current = trigger;
            setSpecialPractice(practice);
          }}
          onPageChange={cardSearch.setPage}
          onChanged={onChanged}
        />
      )}
      <LearningDialogs
        visible={visible}
        theme={theme}
        decks={decks}
        specialPractice={specialPractice}
        specialPracticeTrigger={specialPracticeTrigger}
        libraryDialog={libraryDialog}
        cardAction={cardAction}
        management={management}
        onCloseSpecialPractice={() => setSpecialPractice(undefined)}
        onCloseLibraryDialog={() => setLibraryDialog(undefined)}
        onCreateDeck={(deck) => {
          addDeck(deck);
          chooseDeck(deck.id);
          setLibraryDialog(undefined);
          onChanged();
        }}
        onOpenDeck={(id) => {
          chooseDeck(id);
          setLibraryDialog(undefined);
        }}
        onManageDeck={(mode, deck) => setManagement({ mode, deck })}
        onSyncedAnki={() => setHistoryRevision((value) => value + 1)}
        onCloseCardAction={() => setCardAction(undefined)}
        onCompletedCardAction={(notice) => {
          setCardNotice(notice);
          refreshCardActions();
        }}
        onRefreshCardActions={refreshCardActions}
        onCloseManagement={() => setManagement(null)}
        onRefreshDecks={async () => {
          const latest = await refresh();
          onChanged();
          return latest;
        }}
        onRenamedDeck={(deck) => {
          replaceDeck(deck);
          if (!filters.q.trim()) setMetadataOnlyRevision(revision + 1);
          setManagement(null);
          setLibraryDialog(undefined);
          onChanged();
        }}
        onDeletedDeck={(id) => {
          setLibraryDialog(undefined);
          if (id !== deckId) {
            removeDeck(id);
            setManagement(null);
            onChanged();
            return;
          }
          cardSearch.invalidateResults();
          removeDeck(id);
          const replacement =
            decks.find((deck) => deck.id !== id)?.id || ALL_CARDS;
          setDeckId(replacement);
          review.resetAfterDeletedDeck();
          anki.clearExport();
          setManagement(null);
          onChanged();
        }}
        onChanged={onChanged}
      />
    </section>
  );
}
