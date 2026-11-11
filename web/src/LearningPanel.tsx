import { useProfileValue } from './useProfileValue';
import { useOptionalConfiguration } from './ConfigurationProvider';
import ActionButton from './ActionButton';
import { useCallback, useEffect, useRef, useState } from 'react';
import { request, type Job } from './api';
import { AppError } from './i18n-core';
import { useI18n } from './i18n';
import { useLearningText } from './learning-messages';
import Board, { type Theme } from './Board';
import LearningCardBrowser from './LearningCardBrowser';
import CardSearchControls from './CardSearchControls';
import { SearchFilterSelect } from './SearchFilterForm';
import { useCursorSearch } from './useCursorSearch';
import { ALL_CARDS, cardListPath, emptyCardFilters } from './card-search-api';
import { useCardSearchText } from './card-search-messages';
import { usePositionText } from './position-browser-messages';
import './learning-workspace.css';
import DeckManagementDialog from './DeckManagementDialog';
import { useDeckManagementText } from './deck-management-messages';
import type { VersionedDeck } from './deck-management-api';
import { matchContext } from './boardMath';
import type {
  Deck,
  NextCard,
  CardPage,
  CardQuestion,
  RevealedCard,
  ReviewResult,
  Rating,
} from './learning-api';

const cardKey = (card: CardQuestion) => card.id;

function savedDeck() {
  try {
    return localStorage.getItem('bgtutor.deck') || ALL_CARDS;
  } catch {
    return ALL_CARDS;
  }
}
function rememberDeck(id: string) {
  try {
    if (id) localStorage.setItem('bgtutor.deck', id);
    else localStorage.removeItem('bgtutor.deck');
  } catch {
    /* Selection works without storage. */
  }
}
export function useDecks(revision: number) {
  const [decks, setDecks] = useState<VersionedDeck[]>([]);
  const [ready, setReady] = useState(false);
  const [settledRevision, setSettledRevision] = useState<number>();
  const [error, setError] = useState<unknown>(null);
  const requestId = useRef(0);
  const live = useRef(true);
  async function refresh(signal?: AbortSignal) {
    const id = ++requestId.current;
    try {
      const data = await request<VersionedDeck[]>('/decks', { signal });
      if (live.current && !signal?.aborted && id === requestId.current) {
        setDecks(data);
        setReady(true);
        setSettledRevision(revision);
        setError(null);
      }
      return data;
    } catch (error) {
      if (live.current && !signal?.aborted && id === requestId.current)
        setError(error);
      throw error;
    }
  }
  useEffect(() => {
    live.current = true;
    const abort = new AbortController();
    void refresh(abort.signal).catch(() => {});
    return () => {
      live.current = false;
      abort.abort();
      ++requestId.current;
    };
  }, [revision]);
  const addDeck = (deck: Deck) => {
    ++requestId.current;
    setDecks((previous) =>
      previous.some((item) => item.id === deck.id)
        ? previous
        : [...previous, deck],
    );
  };
  const replaceDeck = (deck: VersionedDeck) => {
    ++requestId.current;
    setDecks((previous) =>
      previous.map((item) => (item.id === deck.id ? deck : item)),
    );
  };
  const removeDeck = (id: string) => {
    ++requestId.current;
    setDecks((previous) => previous.filter((item) => item.id !== id));
  };
  return {
    decks,
    error,
    addDeck,
    replaceDeck,
    removeDeck,
    refresh,
    ready,
    settled: ready && settledRevision === revision,
  };
}
function CreateDeckForm({ onCreated }: { onCreated: (deck: Deck) => void }) {
  const t = useLearningText();
  const { formatError } = useI18n();
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  async function create() {
    setBusy(true);
    setError(null);
    try {
      const deck = await request<Deck>('/decks', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: name.trim() }),
      });
      if (live.current) {
        setName('');
        onCreated(deck);
      }
    } catch (error) {
      if (live.current) setError(error);
    } finally {
      if (live.current) setBusy(false);
    }
  }
  return (
    <form
      className="create-deck"
      onSubmit={(e) => {
        e.preventDefault();
        void create();
      }}
    >
      <input
        aria-label={t('deckName')}
        placeholder={t('deckName')}
        maxLength={120}
        value={name}
        onChange={(e) => setName(e.target.value)}
      />
      <ActionButton
        icon="add"
        label={t('create')}
        compact
        disabled={busy || !name.trim()}
      >
        {t('create')}
      </ActionButton>
      {!!error && <p role="alert">{formatError(error)}</p>}
    </form>
  );
}

export function AddCardsPanel({
  positionId,
  matchId,
  gameNo,
  revision,
  onChanged,
  allowBatch = true,
}: {
  positionId: string | undefined;
  matchId: string;
  gameNo?: number;
  revision: number;
  onChanged: () => void;
  allowBatch?: boolean;
}) {
  const profile = useOptionalConfiguration();
  const t = useLearningText();
  const { formatError, t: baseT } = useI18n();
  const { decks, error: deckError, addDeck, ready } = useDecks(revision);
  const [deckId, setDeckId] = useProfileValue<string>(
    (c) => c.learning.deck_id,
    (c, deck_id) => ({ ...c, learning: { ...c.learning, deck_id } }),
    savedDeck,
  );
  const [minLoss, setMinLossDraft] = useState('0.04');
  const [player, setPlayer] = useProfileValue<string>(
    (c) => c.learning.generation.player,
    (c, player) => ({
      ...c,
      learning: {
        ...c.learning,
        generation: { ...c.learning.generation, player },
      },
    }),
    '',
  );
  useEffect(() => {
    if (profile?.current)
      setMinLossDraft(String(profile.current.learning.generation.min_loss));
  }, [profile?.generation, profile?.ready]);
  const setMinLoss = (value: string) => {
    setMinLossDraft(value);
    if (value.trim() && Number.isFinite(Number(value)) && Number(value) >= 0)
      profile?.updateCurrent((c) => ({
        ...c,
        learning: {
          ...c.learning,
          generation: { ...c.learning.generation, min_loss: Number(value) },
        },
      }));
  };
  const [busy, setBusy] = useState(false);
  const [needsAnalysis, setNeedsAnalysis] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [processed, setProcessed] = useState<number | null>(null);
  const generation = useRef(0);
  useEffect(() => {
    generation.current++;
    setProcessed(null);
    setNeedsAnalysis(false);
    setError(null);
    setBusy(false);
  }, [positionId, matchId, gameNo, allowBatch]);
  useEffect(() => {
    if (!ready || profile) return;
    if (!decks.some((d) => d.id === deckId)) setDeckId(decks[0]?.id || '');
  }, [decks, deckId, ready]);
  useEffect(() => {
    if (deckId && !profile) rememberDeck(deckId);
  }, [deckId]);
  async function add(kind: 'position' | 'match') {
    if (
      !decks.some((deck) => deck.id === deckId) ||
      (kind === 'match' && !allowBatch)
    )
      return;
    const version = ++generation.current;
    setBusy(true);
    setError(null);
    setProcessed(null);
    try {
      let body: Record<string, unknown>;
      if (kind === 'position') {
        body = { position_id: positionId };
      } else {
        let job = gameNo
          ? await request<Job | null>(
              `/matches/${matchId}/analysis?game_no=${gameNo}`,
            )
          : null;
        if (!job || job.status !== 'completed')
          job = await request<Job | null>(`/matches/${matchId}/analysis`);
        if (version !== generation.current) return;
        if (!job || job.status !== 'completed') {
          setNeedsAnalysis(true);
          return;
        }
        body = {
          job_id: job.id,
          game_no: gameNo,
          min_loss: Number(minLoss),
          player: player === '' ? null : Number(player),
        };
      }
      const result = await request<{
        added: number;
        updated: number;
        total: number;
      }>(`/decks/${deckId}/cards`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      if (version === generation.current) {
        setProcessed(result.added + result.updated);
        onChanged();
      }
    } catch (error) {
      if (version === generation.current) setError(error);
    } finally {
      if (version === generation.current) setBusy(false);
    }
  }
  return (
    <section className="add-cards" aria-label={t('addTitle')}>
      <h3>{t('addTitle')}</h3>
      <p>{t('addHint')}</p>
      <div className="learning-controls">
        <label>
          {t('chooseDeck')}{' '}
          <select
            aria-label={t('chooseDeck')}
            value={deckId}
            onChange={(e) => setDeckId(e.target.value)}
            disabled={busy}
          >
            {deckId === ALL_CARDS && (
              <option value={ALL_CARDS} disabled>
                {t('chooseDeck')}
              </option>
            )}
            {!decks.length && <option value="">—</option>}
            {decks.map((deck) => (
              <option value={deck.id} key={deck.id}>
                {deck.name}
              </option>
            ))}
          </select>
        </label>
        <ActionButton
          icon="add"
          label={t('addPosition')}
          compact
          disabled={
            !positionId || !decks.some((deck) => deck.id === deckId) || busy
          }
          onClick={() => void add('position')}
        >
          {t('addPosition')}
        </ActionButton>
      </div>
      <CreateDeckForm
        onCreated={(deck) => {
          addDeck(deck);
          setDeckId(deck.id);
          onChanged();
        }}
      />
      {allowBatch && (
        <div className="learning-controls">
          <label>
            {t('minLoss')}{' '}
            <input
              aria-label={t('minLoss')}
              type="number"
              min={0}
              max={10}
              step="0.01"
              value={minLoss}
              onChange={(e) => setMinLoss(e.target.value)}
            />
          </label>
          <label>
            {t('player')}{' '}
            <select
              aria-label={t('player')}
              value={player}
              onChange={(e) => setPlayer(e.target.value)}
            >
              <option value="">{t('both')}</option>
              <option value="0">{baseT('common.white')}</option>
              <option value="1">{baseT('common.black')}</option>
            </select>
          </label>
          <ActionButton
            icon="add"
            label={
              gameNo
                ? baseT('games.addErrors', { game: gameNo })
                : t('addErrors')
            }
            compact
            disabled={
              !decks.some((deck) => deck.id === deckId) ||
              gameNo === undefined ||
              busy ||
              minLoss === '' ||
              !Number.isFinite(Number(minLoss)) ||
              Number(minLoss) < 0
            }
            onClick={() => void add('match')}
          >
            {gameNo
              ? baseT('games.addErrors', { game: gameNo })
              : t('addErrors')}
          </ActionButton>
        </div>
      )}
      {allowBatch && needsAnalysis && <p role="alert">{t('addHint')}</p>}
      {busy && <p role="status">{t('adding')}</p>}
      {processed !== null && (
        <p role="status">{t('added', { count: processed })}</p>
      )}
      {!!(error || deckError) && (
        <p role="alert">{formatError(error || deckError)}</p>
      )}
    </section>
  );
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
  const profile = useOptionalConfiguration();
  const t = useLearningText();
  const searchT = useCardSearchText();
  const p = usePositionText();
  const [draftFilters, setDraftFilters] = useState(emptyCardFilters);
  const [searchRun, setSearchRun] = useState(0);
  const [filters, setFilters] = useProfileValue(
    (c) => c.learning.filters,
    (c, filters) => ({ ...c, learning: { ...c.learning, filters } }),
    emptyCardFilters,
  );
  useEffect(() => {
    if (profile?.current) setDraftFilters(profile.current.learning.filters);
  }, [profile?.generation, profile?.ready]);
  const managementT = useDeckManagementText();
  const [management, setManagement] = useState<{
    mode: 'rename' | 'delete';
    deck: VersionedDeck;
  } | null>(null);
  const {
    t: uiT,
    locale,
    formatError,
    formatNumber,
    formatEquity,
    formatDecision,
  } = useI18n();
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
  const [deckId, setLocalDeckId] = useState(savedDeck);
  const setDeckId = (deck_id: string) => {
    setLocalDeckId(deck_id);
    profile?.updateCurrent((c) => ({
      ...c,
      learning: { ...c.learning, deck_id },
    }));
  };
  const [appliedDeckToken, setAppliedDeckToken] = useState<number>();
  const [studying, setStudying] = useState(false);
  function chooseDeck(id: string) {
    setDeckId(id);
    setStudying(false);
  }
  useEffect(() => {
    // A profile changes browser defaults, not the context of an active card.
    if (!studying && profile?.current)
      setLocalDeckId(profile.current.learning.deck_id);
  }, [studying, profile?.current?.learning.deck_id, profile?.generation]);
  const [next, setNext] = useState<NextCard | null>(null);
  const [revealed, setRevealed] = useState<RevealedCard | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
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
  const loadCards = useCallback(
    (signal: AbortSignal, after?: string) =>
      request<CardPage>(cardListPath(deckId, filters, after), { signal }),
    [deckId, filters],
  );
  const cardSearch = useCursorSearch({
    scope: listKey,
    enabled: !!deckId,
    loadPage: loadCards,
    itemKey: cardKey,
  });
  useEffect(() => {
    if (!ready || profile) return;
    if (deckId !== ALL_CARDS && !decks.some((d) => d.id === deckId))
      setDeckId(decks[0]?.id || ALL_CARDS);
  }, [decks, deckId, ready]);
  useEffect(() => {
    generation.current++;
    if (ready && !profile) rememberDeck(deckId);
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
    if (!decks.some((deck) => deck.id === requestedDeck.id)) {
      setStudying(false);
      return;
    }
    generation.current++;
    setDeckId(requestedDeck.id);
    setStudying(true);
    setRevealed(null);
    setNext(null);
    setReviewResult(null);
    setError(null);
    setBusy(false);
    setQueueRevision((value) => value + 1);
    setAppliedDeckToken(requestedDeck.token);
  }, [requestedDeck, appliedDeckToken, settled, decks]);
  useEffect(() => {
    if (deckId) setError(null);
  }, [listKey]);
  useEffect(() => {
    if (!deckId || !studying) return;
    const owner = session;
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
        if (!abort.signal.aborted && current.current.session === owner)
          setError(error);
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
      if (version === generation.current) setError(error);
    } finally {
      if (version === generation.current) setBusy(false);
    }
  }
  async function grade(rating: Rating) {
    if (!revealed) return;
    const version = ++generation.current;
    setBusy(true);
    setError(null);
    if (
      postRequest.current?.session !== revealed.session_id ||
      postRequest.current.rating !== rating
    )
      postRequest.current = {
        session: revealed.session_id,
        rating,
        id: crypto.randomUUID(),
      };
    try {
      const result = await request<ReviewResult>(
        `/cards/${revealed.card_id}/review`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            session_id: revealed.session_id,
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
      if (version === generation.current) setError(error);
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
      anchor.download = `${(decks.find((d) => d.id === id)?.name || 'BG Tutor').replace(/[^\p{L}\p{N}_ -]/gu, '_')}.apkg`;
      anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (error) {
      if (current.current.session === owner) setError(error);
    } finally {
      if (current.current.session === owner) setExporting(false);
    }
  }
  function interval(seconds: number) {
    const days = seconds / 86400;
    return formatNumber(
      days >= 1 ? Math.round(days) : Math.max(1, Math.round(seconds / 60)),
      {
        style: 'unit',
        unit: days >= 1 ? 'day' : 'minute',
        unitDisplay: 'short',
      },
    );
  }
  const activeNext = questionOwner === session ? next : null;
  const card = activeNext?.card;
  const context = card ? matchContext(card.position.gnubg_match_id) : null;
  const decision = revealed?.answer.solution.decision;
  const name = decks.find((deck) => deck.id === deckId)?.name;
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
    <SearchFilterSelect
      label={t('chooseDeck')}
      value={deckId}
      onChange={chooseDeck}
    >
      <option value={ALL_CARDS}>{searchT('allCards')}</option>
      {decks.map((deck) => (
        <option value={deck.id} key={deck.id}>
          {deck.name} · {t('cards', { count: deck.card_count })}
        </option>
      ))}
    </SearchFilterSelect>
  );
  const deckPanel = (
    <div className="learning-deck-panel position-deck-panel">
      <h2>{name || p('decks')}</h2>
      <div className="learning-controls">
        {studying && deckSelector}
        <ActionButton
          icon="edit"
          label={managementT('rename')}
          compact
          disabled={!decks.some((deck) => deck.id === deckId)}
          onClick={() => {
            const deck = decks.find((deck) => deck.id === deckId);
            if (deck) setManagement({ mode: 'rename', deck });
          }}
        >
          {managementT('rename')}
        </ActionButton>
        <ActionButton
          icon="delete"
          label={managementT('delete')}
          compact
          disabled={!decks.some((deck) => deck.id === deckId)}
          onClick={() => {
            const deck = decks.find((deck) => deck.id === deckId);
            if (deck) setManagement({ mode: 'delete', deck });
          }}
        >
          {managementT('delete')}
        </ActionButton>
        <ActionButton
          icon="play"
          label={t('studyNow')}
          className="primary"
          disabled={
            !decks.some((deck) => deck.id === deckId) || loading || busy
          }
          onClick={() => {
            generation.current++;
            setRevealed(null);
            setStudying(true);
            setQueueRevision((n) => n + 1);
          }}
        >
          {t('studyNow')}
        </ActionButton>
        <ActionButton
          icon="export"
          label={exporting ? t('exporting') : t('export')}
          compact
          disabled={
            !decks.find((d) => d.id === deckId)?.card_count || exporting
          }
          onClick={() => void download()}
        >
          {exporting ? t('exporting') : t('export')}
        </ActionButton>
      </div>
      <p className="learning-explainer">
        {studying
          ? `${t('separateProgress')} ${t('retention')}`
          : searchT(deckId === ALL_CARDS ? 'chooseScope' : 'scopeHint')}
      </p>
      {!decks.length && <p>{t('empty')}</p>}
    </div>
  );
  const workspaceError = error || cardSearch.error || deckError;
  return (
    <section className="learning-workspace" aria-label={t('study')}>
      <div className="learning-heading">
        <div>
          <h1>{t('title')}</h1>
          <p>{t('description')}</p>
        </div>
        <CreateDeckForm
          onCreated={(deck) => {
            addDeck(deck);
            setDeckId(deck.id);
            onChanged();
          }}
        />
      </div>
      {!!workspaceError && <p role="alert">{formatError(workspaceError)}</p>}
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
      {studying ? (
        <>
          {deckPanel}
          <ActionButton
            icon="back"
            label={t('back')}
            compact
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
          {loading && <p role="status">{t('loading')}</p>}
          {activeNext && (
            <p className="learning-counts">
              {t('due', { count: activeNext.counts.due })} ·{' '}
              {t('newCards', { count: activeNext.counts.new })}
            </p>
          )}
          {activeNext && !card && <p role="status">{t('noDue')}</p>}
          {card && (
            <article className="study-card" data-card-id={card.id}>
              <span className="eyebrow">
                {name} · {t('reviewCount', { count: card.review_count })}
              </span>
              <h3>
                {t(
                  card.kind === 'checker'
                    ? 'questionChecker'
                    : card.kind === 'cube_response'
                      ? 'questionResponse'
                      : 'questionOffer',
                )}
              </h3>
              {context && (
                <p className="study-context">
                  {context.length
                    ? uiT('app.matchLength', { count: context.length })
                    : `${uiT('common.moneyGame')} · Jacoby ${context.jacoby ? '✓' : '✗'}`}{' '}
                  ·{' '}
                  {uiT('app.toMove', {
                    player: card.players[card.position.player],
                  })}
                </p>
              )}
              {card.kind === 'cube_response' && (
                <p>
                  {t('offeredCube', {
                    value: matchContext(card.position.gnubg_match_id).cube * 2,
                  })}
                </p>
              )}
              <Board
                position={card.position}
                white={card.players[0]}
                black={card.players[1]}
                theme={theme}
              />
              {!revealed ? (
                <>
                  <p>{t('answerFirst')}</p>
                  <ActionButton
                    icon="solution"
                    label={t('reveal')}
                    className="primary"
                    disabled={busy}
                    onClick={() => void reveal()}
                  >
                    {t('reveal')}
                  </ActionButton>
                </>
              ) : (
                <section className="study-answer" aria-label={t('solution')}>
                  <h3>{t('solution')}</h3>
                  {decision && (
                    <div className="study-decision">
                      <strong>
                        {t('best')}: {formatDecision(decision.best)}
                      </strong>
                      <p>
                        {t('played')}: {formatDecision(decision.played)} ·{' '}
                        {t('loss')}:{' '}
                        {formatNumber(decision.equity_loss, {
                          minimumFractionDigits: 3,
                          maximumFractionDigits: 3,
                        })}
                      </p>
                    </div>
                  )}
                  {!!revealed.answer.solution.candidates.length && (
                    <table>
                      <thead>
                        <tr>
                          <th>{t('best')}</th>
                          <th>Equity</th>
                        </tr>
                      </thead>
                      <tbody>
                        {revealed.answer.solution.candidates.map(
                          (candidate) => (
                            <tr key={candidate.rank}>
                              <td>{formatDecision(candidate.notation)}</td>
                              <td>{formatEquity(candidate.equity)}</td>
                            </tr>
                          ),
                        )}
                      </tbody>
                    </table>
                  )}
                  <p>
                    {t('engine')}: {revealed.answer.solution.engine} ·{' '}
                    {revealed.answer.solution.plies}-ply
                  </p>
                  {revealed.answer.notes && (
                    <div>
                      <h4>{t('notes')}</h4>
                      <p className="plaintext-comment">
                        {revealed.answer.notes}
                      </p>
                    </div>
                  )}
                  {!!revealed.answer.tags.length && (
                    <p>
                      {t('tags')}: {revealed.answer.tags.join(' · ')}
                    </p>
                  )}
                  <div className="study-ratings">
                    {(['again', 'hard', 'good', 'easy'] as const).map(
                      (rating) => (
                        <ActionButton
                          icon={rating === 'again' ? 'reset' : 'confirm'}
                          label={t(rating)}
                          key={rating}
                          disabled={busy}
                          onClick={() => void grade(rating)}
                        >
                          {t(rating)}
                          <small>{interval(revealed.intervals[rating])}</small>
                        </ActionButton>
                      ),
                    )}
                  </div>
                  {busy && <p role="status">{t('saving')}</p>}
                </section>
              )}
            </article>
          )}
        </>
      ) : (
        <>
          <CardSearchControls
            filters={draftFilters}
            onChange={setDraftFilters}
            onSearch={() => {
              setFilters({ ...draftFilters });
              setSearchRun((value) => value + 1);
            }}
            onClear={() => {
              const empty = emptyCardFilters();
              setDraftFilters(empty);
              setFilters(empty);
              setSearchRun((value) => value + 1);
            }}
          >
            {deckSelector}
          </CardSearchControls>
          <LearningCardBrowser
            key={searchKey}
            visible={visible}
            cards={cardSearch.items}
            total={cardSearch.total ?? cardSearch.items.length}
            loading={cardSearch.loading}
            decks={decks}
            theme={theme}
            hasMore={!!cardSearch.cursor}
            moreLoading={cardSearch.moreLoading}
            onMore={cardSearch.more}
          >
            {deckPanel}
          </LearningCardBrowser>
        </>
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
            onChanged();
          }}
          onDeleted={(id) => {
            generation.current++;
            current.current = { deckId: '', session: {} };
            cardSearch.invalidateResults();
            removeDeck(id);
            const replacement =
              decks.find((deck) => deck.id !== id)?.id || ALL_CARDS;
            if (!profile) rememberDeck(replacement);
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
