import { useEffect, useRef, useState } from 'react';
import { request } from '../../../shared/api/api';
import { ALL_CARDS } from './card-search-api';
import type { VersionedDeck } from './deck-management-api';
import type { Deck } from './learning-api';

/** The deck last chosen in Study, or all cards. */
export function savedDeck() {
  try {
    return localStorage.getItem('neondojo.deck') || ALL_CARDS;
  } catch {
    return ALL_CARDS;
  }
}

/**
 * Loads the decks and reloads them when `revision` changes. Local additions,
 * replacements and removals supersede a response that is still in flight.
 */
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
