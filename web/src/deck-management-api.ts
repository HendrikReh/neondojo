import { request } from './api';
import { AppError } from './i18n-core';
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
