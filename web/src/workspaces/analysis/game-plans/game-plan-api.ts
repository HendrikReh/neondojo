import { request } from '../../../shared/http/api';
import { AppError } from '../../../shared/i18n/i18n-core';
import type { components } from '../../../generated-api';

export type GamePlan = components['schemas']['GamePlan'];
export type GamePlans = Omit<
  components['schemas']['GamePlans'],
  'classifier'
> & {
  classifier: 'opensage_rules_v1_opening1';
};

const planNames = new Set<GamePlan>([
  'opening',
  'pure_race',
  'racing',
  'attacking',
  'priming',
  'anchoring',
]);

/** Absolute White-positive/Black-negative placement; excludes score and mover. */
export function gamePlanBoardKey(board: readonly number[]): string | undefined {
  if (
    board.length !== 26 ||
    Array.from(board).some(
      (count) => !Number.isInteger(count) || Math.abs(count) > 15,
    ) ||
    board[0] > 0 ||
    board[25] < 0 ||
    board.reduce((sum, count) => sum + Math.max(0, count), 0) > 15 ||
    board.reduce((sum, count) => sum + Math.max(0, -count), 0) > 15
  )
    return undefined;
  return JSON.stringify(board);
}

/** Copy only the validated contract so callers cannot mutate cached results. */
export function validateGamePlans(
  value: unknown,
): Readonly<GamePlans> | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const result = value as Partial<GamePlans>;
  if (
    result.classifier !== 'opensage_rules_v1_opening1' ||
    typeof result.source_revision !== 'string' ||
    !result.source_revision.trim() ||
    typeof result.game_over !== 'boolean' ||
    !planNames.has(result.white as GamePlan) ||
    !planNames.has(result.black as GamePlan)
  )
    return undefined;
  return Object.freeze({
    classifier: result.classifier,
    source_revision: result.source_revision,
    game_over: result.game_over,
    white: result.white as GamePlan,
    black: result.black as GamePlan,
  });
}

export async function loadGamePlans(
  board: readonly number[],
  signal?: AbortSignal,
) {
  if (gamePlanBoardKey(board) === undefined)
    throw new AppError('error.invalidResponse');
  const data = await request<unknown>('/game-plans', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ board }),
    signal,
  });
  const result = validateGamePlans(data);
  if (!result) throw new AppError('error.invalidResponse');
  return result;
}

export class GamePlanCache {
  private readonly entries = new Map<string, Readonly<GamePlans>>();
  constructor(private readonly limit = 256) {
    if (!Number.isInteger(limit) || limit < 1)
      throw new RangeError('Invalid cache size');
  }
  get(board: readonly number[]): Readonly<GamePlans> | undefined {
    const key = gamePlanBoardKey(board);
    if (key === undefined) return undefined;
    const stored = this.entries.get(key);
    const result = validateGamePlans(stored);
    if (!result) {
      this.entries.delete(key);
      return undefined;
    }
    this.entries.delete(key);
    this.entries.set(key, result);
    return result;
  }
  set(board: readonly number[], value: unknown) {
    const key = gamePlanBoardKey(board),
      result = validateGamePlans(value);
    if (key === undefined || !result)
      throw new AppError('error.invalidResponse');
    this.entries.delete(key);
    this.entries.set(key, result);
    if (this.entries.size > this.limit)
      this.entries.delete(this.entries.keys().next().value!);
  }
  clear() {
    this.entries.clear();
  }
}

export const gamePlanCache = new GamePlanCache();
