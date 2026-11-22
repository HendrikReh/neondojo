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
import { request } from '../../../shared/api/api';
import { AppError } from '../../../shared/i18n/i18n-core';
import { useI18n } from '../../../shared/i18n/i18n';
import { useLearningText } from './learning-messages';
import type { Theme } from '../../../shared/board/themes';
import StudyCardExercise from './StudyCardExercise';
import LearningCardBrowser from './LearningCardBrowser';
// Analysis imports AddCardsPanel through this module. Keep the re-export at
// this position: it keeps the stylesheets of the Study subtree ahead of the
// Analysis stylesheets in the bundle, which the current cascade relies on.
export { AddCardsPanel } from './AddCardsPanel';
import CardSearchControls from './CardSearchControls';
import { SearchFilterSelect } from '../../../shared/ui/SearchFilterForm';
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
import CardPracticeRound, { roundPath } from './CardPracticeRound';
import CardTagEditor from './CardTagEditor';
import CardTopics from './CardTopics';
import TransferPractice from './TransferPractice';
import CardSpecialPractice from './CardSpecialPractice';
import { usePositionText } from '../../positions/position-browser-messages';
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
import { sendDeckToAnki, syncAnkiReviews } from './learning-api';
import type {
  Deck,
  NextCard,
  CardPage,
  CardQuestion,
  RevealedCard,
  ReviewResult,
  Rating,
  PracticeRound,
  AddCardsResult,
  ImportedAnkiDeck,
} from './learning-api';

const practiceStorageKey = 'neondojo.card-practice-round';
const defaultCardFilters = emptyCardFilters();

function storedPracticeRound() {
  try {
    return localStorage.getItem(practiceStorageKey);
  } catch {
    return null;
  }
}

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
  const [practiceRound, setPracticeRound] = useState<PracticeRound>();
  const [resumeRound, setResumeRound] = useState(storedPracticeRound);
  const [practiceLimit, setPracticeLimit] = useState(10);
  const [specialPractice, setSpecialPractice] = useState<{
    mode: 'pips' | 'score' | 'preview';
    cards: CardQuestion[];
  }>();
  const specialPracticeTrigger = useRef<HTMLButtonElement | null>(null);
  const [practiceStarting, setPracticeStarting] = useState(false);
  const [practiceError, setPracticeError] = useState<unknown>();
  const [ankiImportProgress, setAnkiImportProgress] = useState<{
    file: string;
    current: number;
    total: number;
  }>();
  const [ankiNotice, setAnkiNotice] = useState<string>();
  const [ankiError, setAnkiError] = useState<string>();
  const [sendingAnki, setSendingAnki] = useState(false);
  const [syncingAnki, setSyncingAnki] = useState(false);
  const ankiBusy = !!ankiImportProgress || sendingAnki || syncingAnki;
  const externalAnkiInput = useRef<HTMLInputElement>(null);
  const practiceRequest = useRef<AbortController | null>(null);
  useEffect(() => () => practiceRequest.current?.abort(), []);
  const p = usePositionText();
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
  const { t: uiT, locale, formatError, formatNumber } = useI18n();
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
  const current = useRef({ deckId, session: {} });
  if (current.current.deckId !== deckId) {
    generation.current++;
    current.current = { deckId, session: {} };
  }
  const session = current.current.session;
  const [questionOwner, setQuestionOwner] = useState<object | null>(null);
  const metadataOnlyRevision = useRef<number | null>(null);
  const listRevision = useRef({ revision, value: 0 });
  if (listRevision.current.revision !== revision) {
    if (revision !== metadataOnlyRevision.current) listRevision.current.value++;
    metadataOnlyRevision.current = null;
    listRevision.current.revision = revision;
  }
  const contentRevision = listRevision.current.value;
  const searchKey = JSON.stringify([deckId, filters, searchRun]);
  const listKey = JSON.stringify([searchKey, contentRevision]);
  useEffect(() => {
    if (practiceRequest.current) {
      practiceRequest.current.abort();
      practiceRequest.current = null;
      setPracticeStarting(false);
    }
    setPracticeError(undefined);
  }, [listKey, visible]);
  function rememberRound(id: string | null) {
    setResumeRound(id);
    try {
      if (id) localStorage.setItem(practiceStorageKey, id);
      else localStorage.removeItem(practiceStorageKey);
    } catch {
      // The current round remains usable in this tab.
    }
  }
  async function openPractice(existing?: string) {
    if (practiceRequest.current) return;
    const controller = new AbortController();
    practiceRequest.current = controller;
    setPracticeStarting(true);
    setPracticeError(undefined);
    try {
      const round = await request<PracticeRound>(
        existing
          ? roundPath(existing)
          : cardPracticePath(deckId, filters, practiceLimit),
        { method: existing ? 'GET' : 'POST', signal: controller.signal },
      );
      if (controller.signal.aborted) return;
      rememberRound(round.id);
      setPracticeRound(round);
    } catch (failure) {
      if (controller.signal.aborted) return;
      if (
        existing &&
        failure instanceof AppError &&
        failure.params.status === '404'
      )
        rememberRound(null);
      setPracticeError(failure);
    } finally {
      if (practiceRequest.current === controller) {
        practiceRequest.current = null;
        setPracticeStarting(false);
      }
    }
  }
  async function importAnki(files: File[], external = false) {
    if (!files.length || ankiBusy || (!external && isCollectionScope(deckId)))
      return;
    setAnkiNotice(undefined);
    setAnkiError(undefined);

    const path = external
      ? '/study/anki-imports'
      : `/decks/${deckId}/import.apkg`;
    const failures: string[] = [];
    const importedDecks = new Set<string>();
    let cards = 0;
    let added = 0;
    let existing = 0;

    try {
      for (const [index, file] of files.entries()) {
        setAnkiImportProgress({
          file: file.name,
          current: index + 1,
          total: files.length,
        });
        try {
          const body = new FormData();
          body.set('file', file);
          const options = { method: 'POST', body };
          if (external) {
            const imported = await request<ImportedAnkiDeck[]>(path, options);
            for (const deck of imported) {
              importedDecks.add(deck.name);
              cards += deck.cards;
            }
          } else {
            const result = await request<AddCardsResult>(path, options);
            added += result.added;
            existing += result.updated;
          }
        } catch (failure) {
          failures.push(
            practiceT('ankiImportFailed', {
              file: file.name,
              error: formatError(failure),
            }),
          );
        }
      }

      const successful = files.length - failures.length;
      const summary = [];
      if (files.length > 1) {
        summary.push(
          practiceT('ankiPackagesImported', {
            count: formatNumber(successful),
            total: formatNumber(files.length),
          }),
        );
      }
      if (successful) {
        summary.push(
          external
            ? practiceT('externalAnkiImported', {
                decks: formatNumber(importedDecks.size),
                cards: formatNumber(cards),
              })
            : practiceT('ankiImported', {
                added,
                existing: formatNumber(existing),
              }),
        );
        cardSearch.invalidateResults();
        onChanged();
      }
      if (summary.length) setAnkiNotice(summary.join(' · '));
      if (failures.length) setAnkiError(failures.join('\n'));
    } finally {
      setAnkiImportProgress(undefined);
    }
  }
  async function sendToAnki(deck: Deck) {
    if (ankiBusy) return;
    setSendingAnki(true);
    setAnkiNotice(undefined);
    setAnkiError(undefined);
    try {
      await sendDeckToAnki(deck.id, { locale });
      setAnkiNotice(practiceT('ankiSent', { deck: deck.name }));
    } catch (failure) {
      const status =
        failure instanceof AppError ? failure.params.status : undefined;
      setAnkiError(
        status === '503'
          ? practiceT('ankiUnavailable')
          : status === '504'
            ? practiceT('ankiUnconfirmed')
            : formatError(failure),
      );
    } finally {
      setSendingAnki(false);
    }
  }
  async function syncReviews(deck: Deck) {
    if (ankiBusy) return;
    setSyncingAnki(true);
    setAnkiNotice(undefined);
    setAnkiError(undefined);
    try {
      const result = await syncAnkiReviews(deck.id);
      setAnkiNotice(
        practiceT('ankiReviewsSynced', {
          deck: deck.name,
          profile: result.profile,
          received: result.received_reviews,
          sent: result.sent_reviews,
          matched: result.matched_cards,
          unmatched: formatNumber(result.unmatched_cards),
        }),
      );
    } catch (failure) {
      const status =
        failure instanceof AppError ? failure.params.status : undefined;
      setAnkiError(
        status === '503' ? practiceT('ankiUnavailable') : formatError(failure),
      );
    } finally {
      // Anki may have stopped responding after NeonDojo saved incoming reviews.
      setHistoryRevision((value) => value + 1);
      onChanged();
      setSyncingAnki(false);
    }
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
        if (!abort.signal.aborted && current.current.session === owner) {
          setQuestionOwner(owner);
          setNext(data);
          setError(null);
        }
      })
      .catch((error) => {
        if (abort.signal.aborted || current.current.session !== owner) return;
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
        if (!abort.signal.aborted && current.current.session === owner)
          setLoading(false);
      });
    return () => abort.abort();
  }, [deckId, studying, queueRevision, session]);
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
      const response = await fetch(
        `/api/decks/${id}/export.apkg?locale=${locale}`,
      );
      if (!response.ok) {
        let details = await response.text();
        try {
          details = JSON.parse(details).error || details;
        } catch {
          /* Keep native diagnostic. */
        }
        throw new AppError(
          'error.request',
          { status: response.status },
          details,
        );
      }
      // An explicitly started download completes even when another deck is selected.
      const blob = await response.blob();
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = `${(decks.find((d) => d.id === id)?.name || 'NeonDojo').replace(/[^\p{L}\p{N}_ -]/gu, '_')}.apkg`;
      anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (error) {
      if (current.current.session === owner) setError(error);
    } finally {
      if (current.current.session === owner) setExporting(false);
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
  const cardGeneration = generation.current;
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
          busy={ankiBusy}
          onExport={() => void download()}
          onSend={() => void sendToAnki(sourceDeck)}
          onSync={() => void syncReviews(sourceDeck)}
          onImport={(files) => void importAnki(files)}
        />
      </div>
      {visible && (
        <AnkiAutoSync
          key={sourceDeck.id}
          deckId={sourceDeck.id}
          empty={!sourceDeck.card_count}
          busy={ankiBusy}
          onReceived={() => setHistoryRevision((value) => value + 1)}
        />
      )}
      <p className="learning-explainer">{searchT('scopeHint')}</p>
    </div>
  );
  const workspaceError = error || cardSearch.error || deckError;
  const actionDisabled = !selectedCards.size || cardSearch.loading;
  const canPickTarget = decks.some((deck) => deck.id !== sourceDeck?.id);
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
  const moveLabel = libraryText('move', {
    count: formatNumber(selectedCards.size),
  });
  const moveHint = !ready
    ? undefined
    : decks.length < 2
      ? libraryText('moveNeedsDecks')
      : !sourceDeck
        ? libraryText('moveNeedsSource')
        : undefined;
  const cardActions = (
    <>
      <div
        className="position-selection-summary"
        role="group"
        aria-label={libraryText('actions')}
      >
        <strong aria-live="polite">
          {p('selected', { count: selectedCards.size })}
        </strong>
        <ActionButton
          icon="clear"
          label={p('clearSelection')}
          compact
          disabled={actionDisabled}
          onClick={() => setCardSelection(undefined)}
        />
        <ActionButton
          icon="add"
          label={libraryText('addCards')}
          disabled={actionDisabled || !canPickTarget}
          onClick={() => openCardAction('add')}
        >
          {libraryText('addCards')}
        </ActionButton>
        <ActionButton
          icon="move"
          label={moveLabel}
          disabled={actionDisabled || !sourceDeck || !canPickTarget}
          onClick={() => openCardAction('move')}
        >
          {moveLabel}
        </ActionButton>
        {sourceDeck && (
          <ActionButton
            icon="delete"
            label={libraryText('removeCards')}
            disabled={actionDisabled}
            onClick={() => openCardAction('remove')}
          >
            {libraryText('removeCards')}
          </ActionButton>
        )}
        <ActionButton
          icon="delete"
          label={libraryText('deleteCards')}
          disabled={actionDisabled}
          onClick={() => openCardAction('delete')}
        >
          {libraryText('deleteCards')}
        </ActionButton>
        {moveHint && <small>{moveHint}</small>}
      </div>
      {cardNotice && <p role="status">{cardNotice}</p>}
    </>
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
              void importAnki(files, true);
            }}
          />
          <ActionButton
            icon="open"
            label={practiceT('externalAnkiImport')}
            disabled={ankiBusy}
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
      {ankiImportProgress && (
        <p role="status">
          {practiceT('ankiImporting', {
            file: ankiImportProgress.file,
            current: formatNumber(ankiImportProgress.current),
            total: formatNumber(ankiImportProgress.total),
          })}
        </p>
      )}
      {sendingAnki && <p role="status">{practiceT('sendingAnki')}</p>}
      {syncingAnki && <p role="status">{practiceT('syncingAnkiReviews')}</p>}
      {ankiNotice && <p role="status">{ankiNotice}</p>}
      {ankiError && (
        <p className="learning-anki-error" role="alert">
          {ankiError}
        </p>
      )}
      {!!workspaceError && <p role="alert">{formatError(workspaceError)}</p>}
      {missingDeckToken !== undefined &&
        missingDeckToken === requestedDeck?.token && (
          <p role="alert">{overviewText('deckUnavailable')}</p>
        )}
      {reviewResult && (
        <p className="learning-review-result" role="status">
          {t('nextDue', {
            date: new Intl.DateTimeFormat(locale, {
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
              onAttempt={(result) => {
                if (
                  current.current.session === session &&
                  cardGeneration === generation.current
                ) {
                  setNext((current) =>
                    current?.card?.id === result.revealed.card_id &&
                    current.card.version === result.revealed.version
                      ? { ...current, active_attempt: result }
                      : current,
                  );
                }
              }}
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
                <div className="learning-result-toolbar">
                  <div className="learning-controls learning-practice-row">
                    <SearchFilterSelect
                      label={practiceT('size')}
                      value={String(practiceLimit)}
                      onChange={(value) => setPracticeLimit(Number(value))}
                    >
                      <option value="10">{formatNumber(10)}</option>
                      <option value="20">{formatNumber(20)}</option>
                    </SearchFilterSelect>
                    <ActionButton
                      icon="play"
                      label={practiceT('start')}
                      disabled={
                        practiceStarting ||
                        cardSearch.loading ||
                        !cardSearch.items.length
                      }
                      onClick={() => void openPractice()}
                    >
                      {practiceT('start')}
                    </ActionButton>
                    {resumeRound && (
                      <ActionButton
                        icon="open"
                        label={practiceT('resume')}
                        disabled={practiceStarting}
                        onClick={() => void openPractice(resumeRound)}
                      >
                        {practiceT('resume')}
                      </ActionButton>
                    )}
                    <ActionButton
                      icon="view"
                      label={practiceT('preview')}
                      disabled={cardSearch.loading || !cardSearch.items.length}
                      onClick={(event) => {
                        specialPracticeTrigger.current = event.currentTarget;
                        setSpecialPractice({
                          mode: 'preview',
                          cards: cardSearch.items.slice(0, practiceLimit),
                        });
                      }}
                    >
                      {practiceT('preview')}
                    </ActionButton>
                  </div>
                  {cardActions}
                </div>
                {practiceStarting && <p role="status">{t('loading')}</p>}
                {!!practiceError && (
                  <p role="alert">{formatError(practiceError)}</p>
                )}

                {selectedItem?.position && (
                  <div className="learning-controls">
                    <ActionButton
                      icon="play"
                      label={practiceT('pips')}
                      onClick={(event) => {
                        specialPracticeTrigger.current = event.currentTarget;
                        setSpecialPractice({
                          mode: 'pips',
                          cards: [selectedItem],
                        });
                      }}
                    >
                      {practiceT('pips')}
                    </ActionButton>
                    {selectedItem.kind !== 'checker' && (
                      <ActionButton
                        icon="play"
                        label={practiceT('score')}
                        onClick={(event) => {
                          specialPracticeTrigger.current = event.currentTarget;
                          setSpecialPractice({
                            mode: 'score',
                            cards: [selectedItem],
                          });
                        }}
                      >
                        {practiceT('score')}
                      </ActionButton>
                    )}
                  </div>
                )}
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
            if (!filters.q.trim()) metadataOnlyRevision.current = revision + 1;
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
            current.current = { deckId: '', session: {} };
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
