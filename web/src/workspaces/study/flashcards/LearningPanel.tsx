import { useWorkspaceDefaults } from '../../../settings/configuration/ConfigurationProvider';
import {
  sessionFields,
  sessionString,
  useWorkspaceSession,
} from '../../../shared/hooks/useWorkspaceSession';
import ActionButton from '../../../shared/ui/ActionButton';
import AnkiAutoSync from './AnkiAutoSync';
import AnkiSyncOverview from './AnkiSyncOverview';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useEpoch, useLatest } from '../../../shared/hooks/useLatest';
import { request, requestResponse, saveBlob } from '../../../shared/http/api';
import { AppError, dateTimeFormat } from '../../../shared/i18n/i18n-core';
import { useI18n } from '../../../shared/i18n/i18n';
import { useLearningText } from './learning-messages';
import type { Theme } from '../../../shared/board/themes';
import StudyCardExercise from './StudyCardExercise';
import LearningCardBrowser from './LearningCardBrowser';
import CardSearchControls from './CardSearchControls';
import { useSearchPages } from '../../../shared/hooks/useSearchPages';
import {
  ALL_CARDS,
  CARD_SELECTION_LIMIT,
  CARD_PAGE_SIZE,
  cardListPath,
  cardPracticePath,
  emptyCardFilters,
  isCollectionScope,
  normalizeCardFilters,
} from './card-search-api';
import { useCardSearchText } from './card-search-messages';
import { useCardPracticeText } from './card-practice-messages';
import AnkiTransferStatus from './AnkiTransferStatus';
import CardPracticeActions, {
  type SpecialPractice,
} from './CardPracticeActions';
import CardPracticeRound from './CardPracticeRound';
import CardSelectionActions from './CardSelectionActions';
import { useAnkiTransfers } from './useAnkiTransfers';
import { useCardPractice } from './useCardPractice';
import CardTagEditor from './CardTagEditor';
import CardTopics from './CardTopics';
import TransferPractice from './TransferPractice';
import CardSpecialPractice from './CardSpecialPractice';
import './learning-workspace.css';
import { savedDeck, useDecks } from './useDecks';
import DeckSelector from './DeckSelector';
import AnkiActionsMenu from './AnkiActionsMenu';
import DeckManagementDialog from './DeckManagementDialog';
import CreateDeckForm from './CreateDeckForm';
import {
  DeckLibraryDialog,
  DuplicateCardsDialog,
  CardBatchDialog,
  type CardBatchAction,
} from './DeckLibraryDialogs';
import { useDeckLibraryText } from './deck-library-messages';
import { useStudyOverviewText } from '../study-messages';
import type { VersionedDeck } from './deck-management-api';
import type {
  AttemptedCard,
  NextCard,
  CardPage,
  RevealedCard,
  ReviewResult,
  Rating,
} from './learning-api';

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
  const searchT = useCardSearchText();
  const practiceT = useCardPracticeText();
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
  const libraryText = useDeckLibraryText();
  const [libraryDialog, setLibraryDialog] = useState<
    'decks' | 'duplicates' | 'anki'
  >();
  const [cardAction, setCardAction] = useState<CardBatchAction>();
  const [cardNotice, setCardNotice] = useState<string>();
  const [cardSelection, setCardSelection] = useState<{
    scope: string;
    items: Map<string, CardPage['items'][number]>;
  }>();

  const [management, setManagement] = useState<{
    mode: 'rename' | 'delete';
    deck: VersionedDeck;
  } | null>(null);
  const { t: uiT, locale, formatError } = useI18n();
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
  const [appliedDeckToken, setAppliedDeckToken] = useState<number>();
  const [missingDeckToken, setMissingDeckToken] = useState<number>();
  const [studying, setStudying] = useState(false);
  function chooseDeck(id: string) {
    setMissingDeckToken(undefined);
    setCardNotice(undefined);
    setDeckId(id);
    setStudying(false);
  }
  const [next, setNext] = useState<NextCard | null>(null);
  const [revealed, setRevealed] = useState<RevealedCard | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [attemptBusy, setAttemptBusy] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [reviewResult, setReviewResult] = useState<ReviewResult | null>(null);
  const [queueRevision, setQueueRevision] = useState(0);
  const generation = useRef(0);
  const postRequest = useRef<{
    session: string;
    rating: Rating;
    id: string;
  } | null>(null);
  // Each visit to a deck is one session; late responses belong to an old one.
  const session = useEpoch(deckId);
  const liveSession = useLatest(session);
  const [questionOwner, setQuestionOwner] = useState<number | null>(null);
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
      !studying &&
      !isCollectionScope(deckId) &&
      !decks.some((d) => d.id === deckId)
    )
      setDeckId(ALL_CARDS);
  }, [decks, deckId, settled, studying]);
  useEffect(() => {
    generation.current++;
    setRevealed(null);
    setNext(null);
    setError(null);
    setBusy(false);
    setReviewResult(null);
    setLoading(false);
    setExporting(false);
  }, [deckId]);
  useEffect(() => {
    if (!requestedDeck || appliedDeckToken === requestedDeck.token || !settled)
      return;
    setAppliedDeckToken(requestedDeck.token);
    setPracticeRound(undefined);
    generation.current++;
    setRevealed(null);
    setNext(null);
    setReviewResult(null);
    setError(null);
    setBusy(false);
    setLoading(false);
    if (!decks.some((deck) => deck.id === requestedDeck.id)) {
      setMissingDeckToken(requestedDeck.token);
      setStudying(false);
      return;
    }
    setMissingDeckToken(undefined);
    setDeckId(requestedDeck.id);
    setStudying(true);
    setQueueRevision((value) => value + 1);
  }, [requestedDeck, appliedDeckToken, settled, decks]);
  useEffect(() => {
    if (deckId) setError(null);
  }, [listKey]);
  useEffect(() => {
    if (!deckId || !studying) return;
    const owner = session;
    const entryToken =
      requestedDeck?.id === deckId && requestedDeck.token === appliedDeckToken
        ? appliedDeckToken
        : undefined;
    const abort = new AbortController();
    setLoading(true);
    setNext(null);
    setRevealed(null);
    void request<NextCard>(`/decks/${deckId}/next`, { signal: abort.signal })
      .then((data) => {
        if (!abort.signal.aborted && liveSession.current === owner) {
          setQuestionOwner(owner);
          setNext(data);
          setError(null);
        }
      })
      .catch((error) => {
        if (abort.signal.aborted || liveSession.current !== owner) return;
        if (
          entryToken !== undefined &&
          error instanceof AppError &&
          error.params.status === '404'
        ) {
          setMissingDeckToken(entryToken);
          setReviewResult(null);
          setDeckId(ALL_CARDS);
          setStudying(false);
          onChanged();
        } else setError(error);
      })
      .finally(() => {
        if (!abort.signal.aborted && liveSession.current === owner)
          setLoading(false);
      });
    return () => abort.abort();
  }, [deckId, studying, queueRevision, session]);
  /** Receives the result of the attempt that starts now, unless it is late. */
  function beginAttempt() {
    const version = generation.current;
    return (result: AttemptedCard) => {
      if (liveSession.current !== session || version !== generation.current)
        return;
      setNext((current) =>
        current?.card?.id === result.revealed.card_id &&
        current.card.version === result.revealed.version
          ? { ...current, active_attempt: result }
          : current,
      );
    };
  }
  async function reveal() {
    if (!next?.card) return;
    const version = ++generation.current;
    setBusy(true);
    setError(null);
    try {
      const answer = await request<RevealedCard>(
        `/cards/${next.card.id}/reveal`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ expected_version: next.card.version }),
        },
      );
      if (version === generation.current) setRevealed(answer);
    } catch (error) {
      if (version === generation.current) {
        if (error instanceof AppError && error.params.status === '409')
          reloadCard();
        else setError(error);
      }
    } finally {
      if (version === generation.current) setBusy(false);
    }
  }
  async function grade(answer: RevealedCard, rating: Rating) {
    if (busy) return;
    const version = ++generation.current;
    setBusy(true);
    setError(null);
    if (
      postRequest.current?.session !== answer.session_id ||
      postRequest.current.rating !== rating
    )
      postRequest.current = {
        session: answer.session_id,
        rating,
        id: crypto.randomUUID(),
      };
    try {
      const result = await request<ReviewResult>(
        `/cards/${answer.card_id}/review`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            session_id: answer.session_id,
            request_id: postRequest.current.id,
            rating,
          }),
        },
      );
      if (version === generation.current) {
        setReviewResult(result);
        setRevealed(null);
        setNext(null);
        setQueueRevision((n) => n + 1);
        onChanged();
      }
    } catch (error) {
      if (version === generation.current) {
        if (error instanceof AppError && error.params.status === '409')
          reloadCard();
        else setError(error);
      }
    } finally {
      if (version === generation.current) setBusy(false);
    }
  }
  async function download() {
    const id = deckId;
    const owner = session;
    setExporting(true);
    setError(null);
    try {
      const response = await requestResponse(
        `/decks/${id}/export.apkg?locale=${locale}`,
      );
      // An explicitly started download completes even when another deck is selected.
      saveBlob(
        await response.blob(),
        `${(decks.find((d) => d.id === id)?.name || 'NeonDojo').replace(/[^\p{L}\p{N}_ -]/gu, '_')}.apkg`,
      );
    } catch (error) {
      if (liveSession.current === owner) setError(error);
    } finally {
      if (liveSession.current === owner) setExporting(false);
    }
  }
  function reloadCard() {
    setNext(null);
    setRevealed(null);
    setError(null);
    setQueueRevision((n) => n + 1);
  }
  const activeNext = questionOwner === session ? next : null;
  const card = activeNext?.card;
  const name = sourceDeck?.name;
  if (requestedDeck && requestedDeck.token !== appliedDeckToken)
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
  const deckPanel = sourceDeck && !studying && (
    <div className="learning-deck-panel position-deck-panel">
      <h2>{sourceDeck.name}</h2>
      <div className="learning-controls">
        <ActionButton
          icon="play"
          label={t('studyNow')}
          className="primary"
          disabled={loading || busy || attemptBusy}
          onClick={() => {
            generation.current++;
            setNext(null);
            setRevealed(null);
            setStudying(true);
            setQueueRevision((n) => n + 1);
          }}
        >
          {t('studyNow')}
        </ActionButton>
        <AnkiActionsMenu
          deck={sourceDeck}
          exporting={exporting}
          busy={anki.busy}
          onExport={() => void download()}
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
          onReceived={() => setHistoryRevision((value) => value + 1)}
        />
      )}
      <p className="learning-explainer">{searchT('scopeHint')}</p>
    </div>
  );
  const workspaceError = error || cardSearch.error || deckError;
  const actionDisabled = !selectedCards.size || cardSearch.loading;
  const selectedItem =
    selectedItems.length === 1 ? selectedItems[0] : undefined;
  function openCardAction(mode: CardBatchAction['mode']) {
    if (actionDisabled) return;
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
  const cardActions = (
    <CardSelectionActions
      count={selectedCards.size}
      decks={decks}
      source={sourceDeck}
      ready={ready}
      disabled={actionDisabled}
      notice={cardNotice}
      onClear={() => setCardSelection(undefined)}
      onAction={openCardAction}
    />
  );
  // Reviewing a deck gives the page to the card: the heading and the library
  // actions return with the card search.
  const reviewing = studying && !practiceRound;
  const libraryActions = (
    <div
      className={
        reviewing ? 'learning-controls learning-study-bar' : 'learning-controls'
      }
    >
      {reviewing && (
        <>
          <ActionButton
            icon="back"
            label={t('back')}
            className="subtle"
            onClick={() => {
              setStudying(false);
              setBusy(false);
              setLoading(false);
              generation.current++;
              setRevealed(null);
              setNext(null);
            }}
          >
            {t('back')}
          </ActionButton>
          {deckSelector}
        </>
      )}
      <ActionButton
        icon="open"
        label={libraryText('decks')}
        onClick={() => setLibraryDialog('decks')}
      >
        {libraryText('decks')}
      </ActionButton>
      {reviewing && activeNext && (
        <p className="learning-counts">
          {t('due', { count: activeNext.counts.due })} ·{' '}
          {t('newCards', { count: activeNext.counts.new })}
          {activeNext.counts.new_available != null &&
            activeNext.counts.new > activeNext.counts.new_available && (
              <>
                {' '}
                (
                {t('newToday', {
                  count: activeNext.counts.new_available,
                })}
                )
              </>
            )}
        </p>
      )}
      {!studying && (
        <ActionButton
          icon="search"
          label={libraryText('duplicates')}
          onClick={() => setLibraryDialog('duplicates')}
        >
          {libraryText('duplicates')}
        </ActionButton>
      )}
      {!studying && !practiceRound && (
        <>
          <ActionButton
            icon="refresh"
            label={practiceT('ankiOverview')}
            onClick={() => setLibraryDialog('anki')}
          >
            {practiceT('ankiOverview')}
          </ActionButton>
          <input
            ref={externalAnkiInput}
            type="file"
            hidden
            accept=".apkg"
            multiple
            onChange={(event) => {
              const files = Array.from(event.target.files ?? []);
              event.target.value = '';
              void anki.importPackages(files, true);
            }}
          />
          <ActionButton
            icon="open"
            label={practiceT('externalAnkiImport')}
            disabled={anki.busy}
            onClick={() => externalAnkiInput.current?.click()}
          >
            {practiceT('externalAnkiImport')}
          </ActionButton>
        </>
      )}
    </div>
  );
  return (
    <section className="learning-workspace" aria-label={t('study')}>
      {!reviewing && (
        <div className="learning-heading">
          <div>
            <h1>{t('title')}</h1>
            <p>{t('description')}</p>
          </div>
          {libraryActions}
        </div>
      )}
      {reviewing && libraryActions}
      <AnkiTransferStatus transfers={anki} />
      {!!workspaceError && <p role="alert">{formatError(workspaceError)}</p>}
      {missingDeckToken !== undefined &&
        missingDeckToken === requestedDeck?.token && (
          <p role="alert">{overviewText('deckUnavailable')}</p>
        )}
      {reviewResult && (
        <p className="learning-review-result" role="status">
          {t('nextDue', {
            date: dateTimeFormat(locale, {
              dateStyle: 'medium',
              timeStyle: 'short',
            }).format(new Date(reviewResult.due_at)),
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
      ) : studying ? (
        <>
          {loading && <p role="status">{t('loading')}</p>}
          {activeNext && !card && (
            <p role="status">
              {t(
                activeNext.counts.new > 0 &&
                  activeNext.counts.new_available === 0
                  ? 'dailyLimitReached'
                  : 'noDue',
              )}
            </p>
          )}
          {card && (
            <StudyCardExercise
              key={`${deckId}:${queueRevision}:${card.id}:${card.version}`}
              card={card}
              name={name}
              theme={theme}
              activeAttempt={activeNext?.active_attempt}
              revealed={revealed}
              busy={busy || attemptBusy}
              visible={visible}
              onAttemptStart={beginAttempt}
              onAttemptBusy={setAttemptBusy}
              onConflict={reloadCard}
              onReveal={() => void reveal()}
              onGrade={(answer, rating) => void grade(answer, rating)}
            />
          )}
          <p className="learning-explainer learning-study-note">
            {t('separateProgress')} {t('retention')}
          </p>
        </>
      ) : (
        <>
          <CardSearchControls
            filters={draftFilters}
            onChange={setDraftFilters}
            onSearch={() => {
              setCardNotice(undefined);
              setFilters(normalizeCardFilters(draftFilters));
              setSearchRun((value) => value + 1);
            }}
            onClear={() => {
              setCardNotice(undefined);
              const empty = emptyCardFilters();
              setDraftFilters(empty);
              setFilters(empty);
              setSearchRun((value) => value + 1);
            }}
          >
            {deckSelector}
          </CardSearchControls>
          {deckPanel}
          {/* Secondary study tools stay folded so the cards follow the search. */}
          <details className="learning-extras">
            <summary>{practiceT('studyTools')}</summary>
            <TransferPractice
              onStart={(round) => {
                rememberRound(round.id);
                setPracticeRound(round);
              }}
            />
            <CardTopics
              filters={filters}
              deck={deckId}
              onSelect={(deck, nextFilters) => {
                chooseDeck(deck);
                setDraftFilters(nextFilters);
                setFilters(nextFilters);
                setSearchRun((value) => value + 1);
              }}
            />
          </details>
          <LearningCardBrowser
            key={listKey}
            visible={visible}
            cards={cardSearch.items}
            total={cardSearch.total ?? cardSearch.items.length}
            loading={cardSearch.loading}
            decks={decks}
            selection={{
              ids: selectedCards,
              toggle: toggleCards,
              disabled: cardSearch.loading,
            }}
            actions={
              <>
                <CardPracticeActions
                  cards={cardSearch.items}
                  loading={cardSearch.loading}
                  limit={practiceLimit}
                  onLimit={setPracticeLimit}
                  starting={practiceStarting}
                  error={practiceError}
                  resumable={!!resumeRound}
                  selectedItem={selectedItem}
                  onStart={() => void openPractice()}
                  onResume={() => {
                    if (resumeRound) void openPractice(resumeRound);
                  }}
                  onSpecial={(practice, trigger) => {
                    specialPracticeTrigger.current = trigger;
                    setSpecialPractice(practice);
                  }}
                >
                  {cardActions}
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
            page={cardSearch.page}
            onPageChange={cardSearch.setPage}
          />
        </>
      )}
      {specialPractice && visible && (
        <CardSpecialPractice
          {...specialPractice}
          theme={theme}
          returnFocus={specialPracticeTrigger}
          onClose={() => setSpecialPractice(undefined)}
        />
      )}
      {libraryDialog === 'decks' && (
        <DeckLibraryDialog
          decks={decks}
          close={() => setLibraryDialog(undefined)}
          create={
            <CreateDeckForm
              onCreated={(deck) => {
                addDeck(deck);
                chooseDeck(deck.id);
                setLibraryDialog(undefined);
                onChanged();
              }}
            />
          }
          open={(id) => {
            chooseDeck(id);
            setLibraryDialog(undefined);
          }}
          manage={(mode, deck) => {
            setManagement({ mode, deck });
          }}
        />
      )}
      {libraryDialog === 'duplicates' && (
        <DuplicateCardsDialog
          close={() => setLibraryDialog(undefined)}
          onChanged={onChanged}
        />
      )}
      {libraryDialog === 'anki' && visible && (
        <AnkiSyncOverview
          close={() => setLibraryDialog(undefined)}
          onSynced={() => setHistoryRevision((value) => value + 1)}
        />
      )}
      {cardAction && (
        <CardBatchDialog
          action={cardAction}
          decks={decks}
          close={() => setCardAction(undefined)}
          completed={(notice) => {
            setCardNotice(notice);
            refreshCardActions();
          }}
          refresh={refreshCardActions}
        />
      )}
      {management && (
        <DeckManagementDialog
          mode={management.mode}
          deck={management.deck}
          onClose={() => setManagement(null)}
          onRefresh={async () => {
            const latest = await refresh();
            onChanged();
            return latest;
          }}
          onRenamed={(deck) => {
            replaceDeck(deck);
            if (!filters.q.trim()) setMetadataOnlyRevision(revision + 1);
            setManagement(null);
            setLibraryDialog(undefined);
            onChanged();
          }}
          onDeleted={(id) => {
            setLibraryDialog(undefined);
            if (id !== deckId) {
              removeDeck(id);
              setManagement(null);
              onChanged();
              return;
            }
            generation.current++;
            liveSession.current = -1;
            cardSearch.invalidateResults();
            removeDeck(id);
            const replacement =
              decks.find((deck) => deck.id !== id)?.id || ALL_CARDS;
            setDeckId(replacement);
            setStudying(false);
            setNext(null);
            setRevealed(null);
            setQuestionOwner(null);
            setReviewResult(null);
            setLoading(false);
            setBusy(false);
            setExporting(false);
            setError(null);
            postRequest.current = null;
            if (requestedDeck) setAppliedDeckToken(requestedDeck.token);
            setManagement(null);
            onChanged();
          }}
        />
      )}
    </section>
  );
}
