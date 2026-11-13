import { request } from './api';
import type { components } from './generated-api';
type Schema = components['schemas'];
export type Deck = Schema['Deck'];
export type CardQuestion = Required<Schema['CardQuestion']>;
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
