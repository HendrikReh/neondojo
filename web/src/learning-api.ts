import { request } from './api';
import type { components } from './generated-api';
type Schema = components['schemas'];
export type Deck = Schema['Deck'];
export type CardQuestion = Omit<
  Required<Schema['CardQuestion']>,
  'position' | 'source'
> &
  Pick<Schema['CardQuestion'], 'position' | 'source'>;
export type CardPage = Required<Schema['CardPage']>;
export type NextCard = Required<Schema['NextCard']>;
export type StudyDeck = Schema['StudyDeck'];
export type LearningSummary = Schema['LearningSummary'];
export const loadLearningSummary = (signal: AbortSignal) =>
  request<LearningSummary>('/study/learning-summary', { signal });
export type RevealedCard = Required<Schema['RevealedCard']>;
export type PracticeAnswer = Schema['PracticeAnswer'];
export type AttemptedCard = Schema['AttemptedCard'];
export type CardAttemptRequest = Schema['CardAttemptRequest'];
export type ReviewResult = Schema['ReviewResult'];
export type Rating = Schema['Rating'];

export type CardSuggestion = Schema['CardSuggestion'];
export type CardSuggestions = Schema['CardSuggestions'];
export type SaveSuggestions = Schema['SaveSuggestions'];
export type AddCardsResult = Schema['AddCardsResult'];

const suggestionsPath = (deckId: string) => `/decks/${deckId}/card-suggestions`;

export function loadCardSuggestions(
  deckId: string,
  matchId: string,
  player: string,
  minLoss: number,
  signal: AbortSignal,
) {
  const params = new URLSearchParams({
    match_id: matchId,
    min_loss: String(minLoss),
  });
  if (player !== '') params.set('player', player);
  return request<CardSuggestions>(`${suggestionsPath(deckId)}?${params}`, {
    signal,
  });
}

export function saveCardSuggestions(deckId: string, body: SaveSuggestions) {
  return request<AddCardsResult>(suggestionsPath(deckId), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

export type CardAnswer = Schema['CardAnswer'];
export type PracticeEvaluation = Schema['PracticeEvaluation'];
export type PracticeRound = Schema['PracticeRound'];
export type RoundAnswer = Schema['RoundAnswer'];
export type RoundAdvance = Schema['RoundAdvance'];
export type RoundVersion = Schema['RoundVersion'];
export type CardTopic = Schema['CardTopic'];
export type CardQuery = Schema['CardQuery'];
export type CardTags = Schema['CardTags'];
export type EditCardTags = Schema['EditCardTags'];
export type TransferRequest = Schema['TransferRequest'];
export type TransferSummary = Schema['TransferSummary'];
export type PipResult = Schema['PipResult'];
export type ScoreQuestion = Schema['ScoreQuestion'];
export type ScoreAnswer = Schema['ScoreAnswer'];
export type ScorePractice = Schema['ScorePractice'];
export type ImportedAnkiDeck = Schema['ImportedAnkiDeck'];

export function sendDeckToAnki(deckId: string, body: Schema['ExportQuery']) {
  return request<void>(`/decks/${deckId}/send-to-anki`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

export function syncAnkiReviews(deckId: string) {
  const body: Schema['SyncRequest'] = {};
  return request<Schema['SyncResult']>(`/decks/${deckId}/sync-anki-reviews`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

export type AnkiAutoSyncSettings = Schema['AutoSyncSettings'];
export type LinkedAnkiDeck = Schema['LinkedAnkiDeck'];
export type AnkiSyncAction = Schema['SyncActionRequest']['action'];

export function actOnAnkiSync(deckId: string, action: AnkiSyncAction, retryOf?: string) {
  const body: Schema['SyncActionRequest'] = { action, retry_of: retryOf };
  return request<void>(`/decks/${deckId}/anki-auto-sync/action`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}

export function loadAnkiSyncOverview(signal: AbortSignal) {
  return request<LinkedAnkiDeck[]>('/study/anki-auto-sync', { signal });
}

export type AnkiSyncHistoryEntry = Schema['AnkiSyncHistoryEntry'];
export function loadAnkiSyncHistory(deckId: string, signal: AbortSignal) {
  return request<AnkiSyncHistoryEntry[]>(`/decks/${deckId}/anki-auto-sync/history`, { signal });
}

export function loadAnkiAutoSync(deckId: string, signal: AbortSignal) {
  return request<AnkiAutoSyncSettings>(`/decks/${deckId}/anki-auto-sync`, { signal });
}

export function setAnkiAutoSync(deckId: string, body: Schema['SetAutoSync']) {
  return request<AnkiAutoSyncSettings>(`/decks/${deckId}/anki-auto-sync`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
}
