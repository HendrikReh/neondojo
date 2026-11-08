import type { components } from './generated-api';
type Schema = components['schemas'];
export type Deck = Schema['Deck'];
export type CardQuestion = Required<Schema['CardQuestion']>;
export type CardPage = Required<Schema['CardPage']>;
export type NextCard = Required<Schema['NextCard']>;
export type RevealedCard = Required<Schema['RevealedCard']>;
export type ReviewResult = Schema['ReviewResult'];
export type Rating = Schema['Rating'];
