import { request } from './api';
import type { components } from './generated-api';
type Schema = components['schemas'];
import type { GamePlan } from './game-plan-api';
export const mistakeGamePlans: GamePlan[] = [
  'opening',
  'pure_race',
  'racing',
  'attacking',
  'priming',
  'anchoring',
];
export type MistakeKind = Schema['MistakeKind'];
export type MistakeFilters = {
  player?: string;
  database_id?: string;
  game_plan?: GamePlan;
  min_loss?: number;
  /** Count only matches played without learning aids. */
  independent_only?: boolean;
  /** A match name to translate into the player choice that covers it. */
  resolve?: string;
};
export type MistakeGroup = Schema['MistakeGroup'];
export type MistakePage = Schema['Mistakes'];
export type MistakeReference = Schema['MistakeReference'];
// The generated schema describes kind and players broadly; the endpoint contract
// guarantees one of these question types and fixed White/Black player order.
export type MistakeQuestion = Omit<
  Schema['MistakePracticeQuestion'],
  'kind' | 'players'
> & {
  kind: 'checker' | 'cube_offer' | 'cube_response';
  players: [string, string];
};
export type MistakeAnswer = Schema['MistakeAnswer'];
export type MistakeAttempt = Schema['MistakeAttempt'];
export type MistakeAttemptRequest = Schema['MistakeAttemptRequest'];
export type MistakePracticeRequest = Schema['MistakePracticeRequest'];
export function loadMistakes(filters: MistakeFilters, signal: AbortSignal) {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(filters))
    if (value !== undefined && value !== '') params.set(key, String(value));
  return request<MistakePage>(`/study/mistakes?${params}`, { signal });
}
function post<T>(path: string, body: unknown, signal: AbortSignal) {
  return request<T>(`/study/mistakes/${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal,
  });
}
export const loadMistakePractice = (
  filters: MistakePracticeRequest,
  signal: AbortSignal,
) => post<{ items: MistakeQuestion[] }>('practice', filters, signal);
export const revealMistake = (
  reference: MistakeReference,
  signal: AbortSignal,
) => post<MistakeAnswer>('reveal', reference, signal);
export const checkMistake = (
  input: MistakeAttemptRequest,
  signal: AbortSignal,
) => post<MistakeAttempt>('attempt', input, signal);

export type DecisionHit = Omit<Schema['DecisionHit'], 'kind' | 'players'> & {
  kind: MistakeQuestion['kind'];
  players: [string, string];
};
export type DecisionSearchPage = Omit<Schema['DecisionSearchPage'], 'items'> & {
  items: DecisionHit[];
};
export type DecisionFilters = {
  player: string;
  database_id?: string;
  kind?: Schema['DecisionKind'];
  mistake_kind?: MistakeKind;
  game_plan?: GamePlan;
  min_loss?: number;
  max_loss?: number;
  away?: number;
  opponent_away?: number;
  cube_value?: number;
  cube_owner?: Schema['CubeOwnerFilter'];
  /** Order by loss; the largest loss comes first by default. */
  direction?: Schema['LossDirection'];
  page?: number;
  limit?: number;
};
export function searchDecisions(filters: DecisionFilters, signal: AbortSignal) {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(filters))
    if (value !== undefined && value !== '') params.set(key, String(value));
  return request<DecisionSearchPage>(`/study/decisions?${params}`, { signal });
}
