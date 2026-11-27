import { useEffect, useRef, useState } from 'react';
import { useEpoch, useLatest } from '../../../shared/hooks/useLatest';
import { request } from '../../../shared/http/api';
import { AppError } from '../../../shared/i18n/i18n-core';
import { ALL_CARDS } from './card-search-api';
import type { VersionedDeck } from './deck-management-api';
import type {
  AttemptedCard,
  NextCard,
  Rating,
  RevealedCard,
  ReviewResult,
} from './learning-api';

export type FsrsReviewSession = ReturnType<typeof useFsrsReviewSession>;

/**
 * FSRS review state machine for a study deck: loading the next due card,
 * checking attempts, revealing solutions, submitting ratings, and coordinating
 * external deck requests without letting stale responses cross deck changes.
 */
export function useFsrsReviewSession({
  deckId,
  decks,
  settled,
  requestedDeck,
  onSelectDeck,
  onClearPractice,
  onChanged,
}: {
  deckId: string;
  decks: readonly VersionedDeck[];
  settled: boolean;
  requestedDeck?: { id: string; token: number };
  onSelectDeck: (id: string) => void;
  onClearPractice: () => void;
  onChanged: () => void;
}) {
  const [appliedDeckToken, setAppliedDeckToken] = useState<number>();
  const [missingDeckToken, setMissingDeckToken] = useState<number>();
  const [studying, setStudying] = useState(false);
  const [next, setNext] = useState<NextCard | null>(null);
  const [revealed, setRevealed] = useState<RevealedCard | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [attemptBusy, setAttemptBusy] = useState(false);
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

  useEffect(() => {
    generation.current++;
    setRevealed(null);
    setNext(null);
    setError(null);
    setBusy(false);
    setReviewResult(null);
    setLoading(false);
  }, [deckId]);

  useEffect(() => {
    if (!requestedDeck || appliedDeckToken === requestedDeck.token || !settled)
      return;
    setAppliedDeckToken(requestedDeck.token);
    onClearPractice();
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
    onSelectDeck(requestedDeck.id);
    setStudying(true);
    setQueueRevision((value) => value + 1);
  }, [requestedDeck, appliedDeckToken, settled, decks]);

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
      .catch((failure) => {
        if (abort.signal.aborted || liveSession.current !== owner) return;
        if (
          entryToken !== undefined &&
          failure instanceof AppError &&
          failure.params.status === '404'
        ) {
          setMissingDeckToken(entryToken);
          setReviewResult(null);
          onSelectDeck(ALL_CARDS);
          setStudying(false);
          onChanged();
        } else setError(failure);
      })
      .finally(() => {
        if (!abort.signal.aborted && liveSession.current === owner)
          setLoading(false);
      });
    return () => abort.abort();
  }, [deckId, studying, queueRevision, session]);

  function reloadCard() {
    setNext(null);
    setRevealed(null);
    setError(null);
    setQueueRevision((n) => n + 1);
  }

  function startStudy() {
    generation.current++;
    setNext(null);
    setRevealed(null);
    setStudying(true);
    setQueueRevision((n) => n + 1);
  }

  function stopStudy() {
    setStudying(false);
    setBusy(false);
    setLoading(false);
    generation.current++;
    setRevealed(null);
    setNext(null);
  }

  function leaveStudyForDeckChange() {
    setMissingDeckToken(undefined);
    setStudying(false);
  }

  function resetAfterDeletedDeck() {
    generation.current++;
    liveSession.current = -1;
    setStudying(false);
    setNext(null);
    setRevealed(null);
    setQuestionOwner(null);
    setReviewResult(null);
    setLoading(false);
    setBusy(false);
    setError(null);
    postRequest.current = null;
    if (requestedDeck) setAppliedDeckToken(requestedDeck.token);
  }

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
    } catch (failure) {
      if (version === generation.current) {
        if (failure instanceof AppError && failure.params.status === '409')
          reloadCard();
        else setError(failure);
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
    } catch (failure) {
      if (version === generation.current) {
        if (failure instanceof AppError && failure.params.status === '409')
          reloadCard();
        else setError(failure);
      }
    } finally {
      if (version === generation.current) setBusy(false);
    }
  }

  const activeNext = questionOwner === session ? next : null;

  return {
    session,
    liveSession,
    studying,
    activeNext,
    revealed,
    error,
    setError,
    loading,
    busy,
    attemptBusy,
    setAttemptBusy,
    reviewResult,
    queueRevision,
    waitingForRequestedDeck: Boolean(
      requestedDeck && requestedDeck.token !== appliedDeckToken,
    ),
    missingRequestedDeck:
      missingDeckToken !== undefined &&
      missingDeckToken === requestedDeck?.token,
    startStudy,
    stopStudy,
    leaveStudyForDeckChange,
    resetAfterDeletedDeck,
    reloadCard,
    beginAttempt,
    reveal,
    grade,
  };
}
