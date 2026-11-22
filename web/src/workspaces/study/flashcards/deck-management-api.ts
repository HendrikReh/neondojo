import { request } from '../../../shared/api/api';
import type { components } from '../../../generated-api';
import { AppError } from '../../../shared/i18n/i18n-core';
import type { Deck } from './learning-api';

export type VersionedDeck = Deck;

export function renameDeck(deck: VersionedDeck, name: string) {
  return request<VersionedDeck>(`/decks/${encodeURIComponent(deck.id)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: name.trim(), expected_version: deck.version }),
  });
}
export function deleteDeck(deck: VersionedDeck) {
  return request<void>(
    `/decks/${encodeURIComponent(deck.id)}?expected_version=${deck.version}`,
    { method: 'DELETE' },
  );
}
export function isDeckConflict(error: unknown) {
  return (
    error instanceof AppError &&
    ['404', '409'].includes(String(error.params.status))
  );
}

type Schema = components['schemas'];
export type DuplicateCard = Schema['DuplicateCard'];
export type DuplicateDeck = Schema['DuplicateDeck'];

function post<T>(path: string, body: unknown) {
  return request<T>(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

export function moveDeckCards(
  source: VersionedDeck,
  target: VersionedDeck,
  cardIds: string[],
) {
  return post<Schema['MoveCardsResult']>(`/decks/${source.id}/move-cards`, {
    source_version: source.version,
    target_id: target.id,
    target_version: target.version,
    card_ids: cardIds,
  } satisfies Schema['MoveCards']);
}

export function addDeckCards(deck: VersionedDeck, cardIds: string[]) {
  return post<Schema['AddDeckCardsResult']>(`/decks/${deck.id}/add-cards`, {
    expected_version: deck.version,
    card_ids: cardIds,
  } satisfies Schema['AddDeckCards']);
}

export function removeDeckCards(deck: DuplicateDeck, cardIds: string[]) {
  return post<void>(`/decks/${deck.id}/remove-cards`, {
    expected_version: deck.version,
    card_ids: cardIds,
  } satisfies Schema['RemoveCards']);
}

export function deleteCards(cards: Schema['DeleteCard'][]) {
  return post<void>('/cards/delete', { cards } satisfies Schema['DeleteCards']);
}

export type DuplicatePeer = Schema['DuplicatePeer'];

export function loadDuplicates(signal: AbortSignal, page: number) {
  return request<Schema['DuplicatePage']>(`/cards/duplicates?page=${page}`, {
    signal,
  });
}

export function resolveDuplicate(keep: DuplicatePeer, remove: DuplicateCard) {
  return post<void>('/cards/duplicates/resolve', {
    keep_id: keep.id,
    keep_version: keep.version,
    delete_id: remove.card.id,
    delete_version: remove.card.version,
  } satisfies Schema['ResolveDuplicate']);
}
