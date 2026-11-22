import { request } from '../api/api';
import { AppError } from '../i18n/i18n-core';
import type { components } from '../../generated-api';

export type CubeContext = components['schemas']['CubeContext'];
export type CubeEvaluation = components['schemas']['CubeEvaluation'];

const cache = new Map<string, Readonly<CubeContext>>();
const CACHE_LIMIT = 200;

const fraction = (value: unknown): value is number =>
  typeof value === 'number' && value >= 0 && value <= 1;

function validate(value: unknown): Readonly<CubeContext> {
  const context = value as Partial<CubeContext> | null;
  if (
    !context ||
    !Number.isInteger(context.match_length) ||
    !Number.isInteger(context.cube_value) ||
    (context.take_point != null && !fraction(context.take_point.value))
  )
    throw new AppError('error.invalidResponse');
  return Object.freeze(context as CubeContext);
}

/** The take point depends only on score, cube and who doubles. */
export async function loadCubeContext(
  matchId: string,
  doubler: number,
  signal?: AbortSignal,
): Promise<Readonly<CubeContext>> {
  const key = `${matchId}:${doubler}`;
  const cached = cache.get(key);
  if (cached) return cached;
  const context = validate(
    await request<unknown>('/cube-context', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ match_id: matchId, doubler }),
      signal,
    }),
  );
  if (cache.size >= CACHE_LIMIT)
    cache.delete(cache.keys().next().value as string);
  cache.set(key, context);
  return context;
}

/**
 * A doubler is too good when the opponent would pass and playing on is still
 * worth more than cashing. A responder's saved values are negated.
 */
export function tooGoodToDouble(
  evaluation: CubeEvaluation,
  response: boolean,
): boolean {
  const sign = response ? -1 : 1;
  const [noDouble, take, pass] = [
    sign * evaluation.no_double,
    sign * evaluation.double_take,
    sign * evaluation.double_pass,
  ];
  return pass <= take && noDouble > pass;
}

export type CubeOutcome = 'no_double' | 'double_take' | 'double_pass';

/** The outcome reached when both sides act correctly, from either side. */
export function properCubeOutcome(
  evaluation: CubeEvaluation,
  response: boolean,
): CubeOutcome {
  // Compare from the doubler's side: the opponent picks the smaller value.
  const sign = response ? -1 : 1;
  const [noDouble, take, pass] = [
    sign * evaluation.no_double,
    sign * evaluation.double_take,
    sign * evaluation.double_pass,
  ];
  const takes = take <= pass;
  if (Math.min(take, pass) <= noDouble) return 'no_double';
  return takes ? 'double_take' : 'double_pass';
}

export type CubeChances = components['schemas']['CubeChances'];
export type CubePosition = components['schemas']['Position'];

/**
 * Chances of a cube decision whose saved analysis has none, calculated now
 * with the practice engine. They carry their own engine and depth.
 */
export async function calculateCubeChances(
  position: CubePosition,
  response: boolean,
  signal?: AbortSignal,
): Promise<CubeChances> {
  const result = await request<CubeChances>('/cube-chances', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ position, response }),
    signal,
  });
  if (
    !Array.isArray(result?.probabilities) ||
    result.probabilities.length !== 6 ||
    !result.probabilities.every(fraction)
  )
    throw new AppError('error.invalidResponse');
  return result;
}
