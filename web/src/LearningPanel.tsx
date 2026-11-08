import { useEffect, useRef, useState } from 'react';
import { request, type Job } from './api';
import { AppError } from './i18n-core';
import { useI18n } from './i18n';
import { useLearningText } from './learning-messages';
import Board, { type Theme } from './Board';
import { matchContext } from './boardMath';
import type {
  Deck,
  NextCard,
  CardPage,
  RevealedCard,
  ReviewResult,
  Rating,
} from './learning-api';

function savedDeck() {
  try {
    return localStorage.getItem('bgtutor.deck') || '';
  } catch {
    return '';
  }
}
function rememberDeck(id: string) {
  try {
    localStorage.setItem('bgtutor.deck', id);
  } catch {
    /* Selection works without storage. */
  }
}
function useDecks(revision: number) {
  const [decks, setDecks] = useState<Deck[]>([]);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<unknown>(null);
  useEffect(() => {
    const abort = new AbortController();
    void request<Deck[]>('/decks', { signal: abort.signal })
      .then((data) => {
        if (!abort.signal.aborted) {
          setDecks(data);
          setReady(true);
          setError(null);
        }
      })
      .catch((error) => {
        if (!abort.signal.aborted) setError(error);
      });
    return () => abort.abort();
  }, [revision]);
  const addDeck = (deck: Deck) =>
    setDecks((previous) =>
      previous.some((item) => item.id === deck.id)
        ? previous
        : [...previous, deck],
    );
  return { decks, error, addDeck, ready };
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
      <button disabled={busy || !name.trim()}>{t('create')}</button>
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
}: {
  positionId: string | undefined;
  matchId: string;
  gameNo?: number;
  revision: number;
  onChanged: () => void;
}) {
  const t = useLearningText();
  const { formatError, t: baseT } = useI18n();
  const { decks, error: deckError, addDeck, ready } = useDecks(revision);
  const [deckId, setDeckId] = useState(savedDeck);
  const [minLoss, setMinLoss] = useState('0.04');
  const [player, setPlayer] = useState('');
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
  }, [positionId, matchId, gameNo]);
  useEffect(() => {
    if (!ready) return;
    if (!decks.some((d) => d.id === deckId)) setDeckId(decks[0]?.id || '');
  }, [decks, deckId, ready]);
  useEffect(() => {
    if (deckId) rememberDeck(deckId);
  }, [deckId]);
  async function add(kind: 'position' | 'match') {
    if (!deckId) return;
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
            {!decks.length && <option value="">—</option>}
            {decks.map((deck) => (
              <option value={deck.id} key={deck.id}>
                {deck.name}
              </option>
            ))}
          </select>
        </label>
        <button
          disabled={!positionId || !deckId || busy}
          onClick={() => void add('position')}
        >
          {t('addPosition')}
        </button>
      </div>
      <CreateDeckForm
        onCreated={(deck) => {
          addDeck(deck);
          setDeckId(deck.id);
          onChanged();
        }}
      />
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
        <button
          disabled={
            !deckId ||
            gameNo === undefined ||
            busy ||
            minLoss === '' ||
            !Number.isFinite(Number(minLoss)) ||
            Number(minLoss) < 0
          }
          onClick={() => void add('match')}
        >
          {gameNo ? baseT('games.addErrors', { game: gameNo }) : t('addErrors')}
        </button>
      </div>
      {needsAnalysis && <p role="alert">{t('addHint')}</p>}
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
  theme,
  revision,
  onChanged,
  requestedDeck,
}: {
  theme: Theme;
  revision: number;
  onChanged: () => void;
  requestedDeck?: { id: string; token: number };
}) {
  const t = useLearningText();
  const {
    t: uiT,
    locale,
    formatError,
    formatNumber,
    formatEquity,
    formatDecision,
  } = useI18n();
  const { decks, error: deckError, addDeck, ready } = useDecks(revision);
  const [deckId, setDeckId] = useState(savedDeck);
  const [appliedDeckToken, setAppliedDeckToken] = useState<number>();
  const [studying, setStudying] = useState(false);
  const [next, setNext] = useState<NextCard | null>(null);
  const [revealed, setRevealed] = useState<RevealedCard | null>(null);
  const [page, setPage] = useState<CardPage>({ items: [], next_cursor: null });
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
  const current = useRef(deckId);
  current.current = deckId;
  useEffect(() => {
    if (!ready) return;
    if (!decks.some((d) => d.id === deckId)) setDeckId(decks[0]?.id || '');
  }, [decks, deckId, ready]);
  useEffect(() => {
    generation.current++;
    if (deckId) rememberDeck(deckId);
    setRevealed(null);
    setNext(null);
    setError(null);
    setBusy(false);
    setReviewResult(null);
    setLoading(false);
    setPage({ items: [], next_cursor: null });
  }, [deckId]);
  useEffect(() => {
    if (
      !requestedDeck ||
      appliedDeckToken === requestedDeck.token ||
      !ready ||
      !decks.some((deck) => deck.id === requestedDeck.id)
    )
      return;
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
  }, [requestedDeck, appliedDeckToken, ready, decks]);
  useEffect(() => {
    if (!deckId) return;
    const abort = new AbortController();
    void request<CardPage>(`/decks/${deckId}/cards?limit=50`, {
      signal: abort.signal,
    })
      .then((data) => {
        if (!abort.signal.aborted) setPage(data);
      })
      .catch((error) => {
        if (!abort.signal.aborted) setError(error);
      });
    return () => abort.abort();
  }, [deckId, revision]);
  useEffect(() => {
    if (!deckId || !studying) return;
    const abort = new AbortController();
    setLoading(true);
    setNext(null);
    setRevealed(null);
    void request<NextCard>(`/decks/${deckId}/next`, { signal: abort.signal })
      .then((data) => {
        if (!abort.signal.aborted) {
          setNext(data);
          setError(null);
        }
      })
      .catch((error) => {
        if (!abort.signal.aborted) setError(error);
      })
      .finally(() => {
        if (!abort.signal.aborted) setLoading(false);
      });
    return () => abort.abort();
  }, [deckId, studying, queueRevision]);
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
  async function more() {
    if (!page.next_cursor) return;
    const id = deckId;
    try {
      const data = await request<CardPage>(
        `/decks/${id}/cards?limit=50&after=${page.next_cursor}`,
      );
      if (current.current === id)
        setPage((old) => ({ ...data, items: [...old.items, ...data.items] }));
    } catch (error) {
      if (current.current === id) setError(error);
    }
  }
  async function download() {
    const id = deckId;
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
      const url = URL.createObjectURL(await response.blob());
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = `${(decks.find((d) => d.id === id)?.name || 'BG Tutor').replace(/[^\p{L}\p{N}_ -]/gu, '_')}.apkg`;
      anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (error) {
      if (current.current === id) setError(error);
    } finally {
      setExporting(false);
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
  const card = next?.card;
  const context = card ? matchContext(card.position.gnubg_match_id) : null;
  const decision = revealed?.answer.solution.decision;
  const name = decks.find((deck) => deck.id === deckId)?.name;
  if (requestedDeck && requestedDeck.token !== appliedDeckToken)
    return (
      <section className="learning-workspace" aria-label={t('study')}>
        <h2>{t('study')}</h2>
        <p role="status">{t('loading')}</p>
        {!!deckError && (
          <>
            <p role="alert">{formatError(deckError)}</p>
            <button onClick={onChanged}>{uiT('games.retry')}</button>
          </>
        )}
      </section>
    );
  return (
    <section className="learning-workspace" aria-label={t('study')}>
      <div className="learning-heading">
        <div>
          <h2>{t('title')}</h2>
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
      <div className="learning-controls">
        <label>
          {t('chooseDeck')}{' '}
          <select
            aria-label={t('chooseDeck')}
            value={deckId}
            onChange={(e) => {
              setDeckId(e.target.value);
              setStudying(false);
            }}
          >
            {!decks.length && <option value="">—</option>}
            {decks.map((deck) => (
              <option value={deck.id} key={deck.id}>
                {deck.name} · {t('cards', { count: deck.card_count })}
              </option>
            ))}
          </select>
        </label>
        <button
          className="primary"
          disabled={!deckId || loading || busy}
          onClick={() => {
            generation.current++;
            setRevealed(null);
            setStudying(true);
            setQueueRevision((n) => n + 1);
          }}
        >
          {t('studyNow')}
        </button>
        <button
          disabled={
            !deckId ||
            !decks.find((d) => d.id === deckId)?.card_count ||
            exporting
          }
          onClick={() => void download()}
        >
          {exporting ? t('exporting') : t('export')}
        </button>
      </div>
      <p className="learning-explainer">
        {t('separateProgress')} {t('retention')}
      </p>
      {!!(error || deckError) && (
        <p role="alert">{formatError(error || deckError)}</p>
      )}
      {!decks.length && <p>{t('empty')}</p>}
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
          <button
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
          </button>
          {loading && <p role="status">{t('loading')}</p>}
          {next && (
            <p className="learning-counts">
              {t('due', { count: next.counts.due })} ·{' '}
              {t('newCards', { count: next.counts.new })}
            </p>
          )}
          {next && !card && <p role="status">{t('noDue')}</p>}
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
                  <button
                    className="primary"
                    disabled={busy}
                    onClick={() => void reveal()}
                  >
                    {t('reveal')}
                  </button>
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
                        <button
                          key={rating}
                          disabled={busy}
                          onClick={() => void grade(rating)}
                        >
                          {t(rating)}
                          <small>{interval(revealed.intervals[rating])}</small>
                        </button>
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
        !!page.items.length && (
          <div className="learning-card-list">
            <h3>{t('cardList')}</h3>
            <table>
              <thead>
                <tr>
                  <th>{t('question')}</th>
                  <th>{t('reviews')}</th>
                </tr>
              </thead>
              <tbody>
                {page.items.map((card) => (
                  <tr key={card.id}>
                    <td>
                      {t(
                        card.kind === 'checker'
                          ? 'questionChecker'
                          : card.kind === 'cube_response'
                            ? 'questionResponse'
                            : 'questionOffer',
                      )}
                      <small>{card.position.position_id}</small>
                    </td>
                    <td>{formatNumber(card.review_count)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {page.next_cursor && (
              <button onClick={() => void more()}>{t('more')}</button>
            )}
          </div>
        )
      )}
    </section>
  );
}
