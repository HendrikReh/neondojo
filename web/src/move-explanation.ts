import { request } from './api';
import type { components } from './generated-api';
import { AppError } from './i18n-core';

export type PlayFacts = components['schemas']['PlayFacts'];
export type MoveExplanation = components['schemas']['MoveExplanation'];
export type ExplainedPosition = {
  board: readonly number[];
  player: number;
  dice: readonly number[];
};

const cache = new Map<string, Readonly<MoveExplanation>>();
const CACHE_LIMIT = 200;

export async function loadMoveExplanation(
  position: ExplainedPosition,
  best: string,
  played: string,
  signal?: AbortSignal,
): Promise<Readonly<MoveExplanation>> {
  const body = JSON.stringify({
    board: position.board,
    player: position.player,
    dice: position.dice,
    best,
    played,
  });
  const cached = cache.get(body);
  if (cached) return cached;
  const result = await request<MoveExplanation>('/move-explanations', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body,
    signal,
  });
  if (!Array.isArray(result?.best?.points) || !result.played)
    throw new AppError('error.invalidResponse');
  if (cache.size >= CACHE_LIMIT)
    cache.delete(cache.keys().next().value as string);
  cache.set(body, Object.freeze(result));
  return result;
}

export type Difference =
  | { kind: 'hits' | 'shots' | 'prime' | 'blots'; best: number; played: number }
  | { kind: 'pointsBest' | 'pointsPlayed'; points: number[] }
  | { kind: 'gammon' | 'gammonLoss'; best: number; played: number };

/** Chances as win, gammon, backgammon, then the three losing chances. */
type Chances = readonly number[] | null | undefined;

/** Sources store chances loosely; accept only six finite numbers. */
export function sixChances(value: unknown): number[] | undefined {
  return Array.isArray(value) &&
    value.length === 6 &&
    value.every((entry) => typeof entry === 'number' && Number.isFinite(entry))
    ? (value as number[])
    : undefined;
}

/**
 * The few facts that differ, most telling first: checkers hit, shots
 * allowed, points held, then structure and gammon chances.
 */
export function moveDifferences(
  explanation: MoveExplanation,
  bestChances?: Chances,
  playedChances?: Chances,
  limit = 3,
): Difference[] {
  const { best, played } = explanation;
  const found: Difference[] = [];
  const number = (
    kind: 'hits' | 'shots' | 'prime' | 'blots',
    a: number,
    b: number,
  ) => {
    if (a !== b) found.push({ kind, best: a, played: b });
  };
  number('hits', best.opponent_on_bar, played.opponent_on_bar);
  number('shots', best.hitting_rolls, played.hitting_rolls);
  const only = (a: number[], b: number[]) =>
    a.filter((point) => !b.includes(point));
  const bestOnly = only(best.points, played.points);
  const playedOnly = only(played.points, best.points);
  if (bestOnly.length) found.push({ kind: 'pointsBest', points: bestOnly });
  if (playedOnly.length)
    found.push({ kind: 'pointsPlayed', points: playedOnly });
  number('prime', best.longest_prime, played.longest_prime);
  // Shots already say what the blots cost; mention the count only otherwise.
  if (best.hitting_rolls === played.hitting_rolls)
    number('blots', best.blots.length, played.blots.length);
  if (bestChances?.length === 6 && playedChances?.length === 6) {
    for (const [kind, index] of [
      ['gammon', 1],
      ['gammonLoss', 4],
    ] as const)
      if (Math.abs(bestChances[index] - playedChances[index]) >= 0.01)
        found.push({
          kind,
          best: bestChances[index],
          played: playedChances[index],
        });
  }
  return found.slice(0, limit);
}
