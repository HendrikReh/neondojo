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
