import type { components } from './generated-api';

export type CheckerTurn = components['schemas']['CheckerTurn'];
export type CheckerStep = components['schemas']['PlayStep'];
export interface CheckerStepWithDie extends CheckerStep {
  die: number;
}
export type BearoffDirection = 'left' | 'right';
export function matchingMoves<T extends Pick<CheckerTurn, 'steps'>>(
  moves: T[],
  prefix: CheckerStep[],
) {
  return moves.filter((move) =>
    prefix.every(
      (step, index) =>
        move.steps[index]?.from === step.from &&
        move.steps[index]?.to === step.to,
    ),
  );
}
export function nextPoints(
  moves: Pick<CheckerTurn, 'steps'>[],
  prefix: CheckerStep[],
  source: number | null,
) {
  return [
    ...new Set(
      matchingMoves(moves, prefix).flatMap((move) => {
        const next = move.steps[prefix.length];
        return !next
          ? []
          : source === null
            ? [next.from]
            : next.from === source
              ? [next.to]
              : [];
      }),
    ),
  ];
}
export function applySteps(
  board: number[],
  steps: CheckerStep[],
  player: number,
) {
  const result = [...board],
    sign = player === 0 ? 1 : -1;
  for (const { from, to } of steps) {
    result[from] -= sign;
    if (to < 0 || to > 25) continue;
    if (result[to] === -sign) {
      result[to] = 0;
      result[player === 0 ? 0 : 25] -= sign;
    }
    result[to] += sign;
  }
  return result;
}
export function boardFlips(
  human: number,
  direction: BearoffDirection,
): [number, number] {
  return [direction === 'left' ? -1 : 1, human === 1 ? -1 : 1];
}
export function relativePoint(point: number, human: number) {
  return human === 0 ? point : 25 - point;
}

export function matchingCheckerTurns<T extends CheckerTurn>(
  turns: T[],
  prefix: CheckerStepWithDie[],
) {
  return matchingMoves(turns, prefix).filter((turn) =>
    prefix.every((step, index) => step.die === turn.dice[index]),
  );
}

/** Keep the preferred die where the clicked checker can legally use it. */
export function nextCheckerSteps(
  turns: CheckerTurn[],
  index: number,
  preferredDie: number,
) {
  const choices = new Map<number, CheckerStepWithDie>();
  for (const turn of turns) {
    const step = turn.steps[index];
    if (!step) continue;
    const die = turn.dice[index];
    if (!choices.has(step.from) || die === preferredDie)
      choices.set(step.from, { ...step, die });
  }
  return choices;
}
