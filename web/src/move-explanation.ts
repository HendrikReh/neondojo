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

type Counted = 'hits' | 'shots' | 'prime' | 'blots' | 'deep' | 'stack';
type Chance = 'win' | 'gammon' | 'gammonLoss';
/** Which play a fact speaks for. */
type Favors = { favors: 'best' | 'played' };
export type Difference = Favors &
  (
    | { kind: Exclude<Counted, 'shots'>; best: number; played: number }
    | {
        kind: 'shots';
        best: number;
        played: number;
        /** Pips lost to a hit, averaged over the opponent's 36 rolls. */
        bestCost: number;
        playedCost: number;
      }
    | { kind: 'pointsBest' | 'pointsPlayed'; points: number[] }
    | { kind: Chance; best: number; played: number }
  );

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
 * The facts that differ. Those that speak for the best play come first, so
 * the explanation shows what the better play gains before what it concedes:
 * winning chances, checkers hit, the cost of being hit, points and structure.
 */
export function moveDifferences(
  explanation: MoveExplanation,
  bestChances?: Chances,
  playedChances?: Chances,
  limit = { best: 3, played: 2 },
): Difference[] {
  const { best, played } = explanation;
  const found: Difference[] = [];
  const favors = (bestIsBetter: boolean): Favors => ({
    favors: bestIsBetter ? 'best' : 'played',
  });
  const chances =
    bestChances?.length === 6 && playedChances?.length === 6
      ? ([bestChances, playedChances] as const)
      : undefined;
  const chance = (kind: Chance, index: number, higherIsBetter: boolean) => {
    if (!chances) return;
    const [a, b] = [chances[0][index], chances[1][index]];
    if (Math.abs(a - b) >= 0.01)
      found.push({
        kind,
        best: a,
        played: b,
        ...favors(a > b === higherIsBetter),
      });
  };
  const count = (
    kind: Exclude<Counted, 'shots'>,
    a: number,
    b: number,
    higherIsBetter: boolean,
  ) => {
    if (a !== b)
      found.push({
        kind,
        best: a,
        played: b,
        ...favors(a > b === higherIsBetter),
      });
  };
  chance('win', 0, true);
  count('hits', best.opponent_on_bar, played.opponent_on_bar, true);
  if (
    best.hitting_rolls !== played.hitting_rolls ||
    best.hit_pips !== played.hit_pips
  )
    found.push({
      kind: 'shots',
      best: best.hitting_rolls,
      played: played.hitting_rolls,
      bestCost: best.hit_pips / 36,
      playedCost: played.hit_pips / 36,
      // A blot deep in the opponent's home board is cheap to lose, so the
      // pips at stake decide; the number of rolls only breaks a tie.
      ...favors(
        best.hit_pips !== played.hit_pips
          ? best.hit_pips < played.hit_pips
          : best.hitting_rolls < played.hitting_rolls,
      ),
    });
  const only = (a: number[], b: number[]) =>
    a.filter((point) => !b.includes(point));
  const bestOnly = only(best.points, played.points);
  const playedOnly = only(played.points, best.points);
  if (bestOnly.length)
    found.push({ kind: 'pointsBest', points: bestOnly, favors: 'best' });
  if (playedOnly.length)
    found.push({ kind: 'pointsPlayed', points: playedOnly, favors: 'played' });
  count('prime', best.longest_prime, played.longest_prime, true);
  count('deep', best.deep_checkers, played.deep_checkers, false);
  count('stack', best.stacked_checkers, played.stacked_checkers, false);
  // Shots already say what the blots cost; mention the count only otherwise.
  if (
    best.hitting_rolls === played.hitting_rolls &&
    best.hit_pips === played.hit_pips
  )
    count('blots', best.blots.length, played.blots.length, false);
  chance('gammon', 1, true);
  chance('gammonLoss', 4, false);
  const side = (favored: Favors['favors'], most: number) =>
    found.filter((difference) => difference.favors === favored).slice(0, most);
  return [...side('best', limit.best), ...side('played', limit.played)];
}
