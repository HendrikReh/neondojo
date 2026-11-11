export interface CheckerStep {
  from: number;
  to: number;
}
export interface LegalPlay {
  notation: string;
  steps: CheckerStep[];
}
export type BearoffDirection = 'left' | 'right';
export function matchingMoves<T extends Pick<LegalPlay, 'steps'>>(
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
  moves: Pick<LegalPlay, 'steps'>[],
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

export interface OrderedLegalTurn extends LegalPlay {
  dice: number[];
  moveIndex: number;
}

/** UI dice-step routes are accepted only when their complete result is one of
 * GNUbg's legal final boards. Native move ordering/deduplication must not force
 * the user to move a different checker or combine two dice into a single click.
 */
export function orderedLegalTurns(
  board: number[],
  player: number,
  dice: number[],
  nativeMoves: LegalPlay[],
): OrderedLegalTurn[] {
  if (!nativeMoves.length) return [];
  const nativeBoards = new Map<string, number>();
  nativeMoves.forEach((move, index) => {
    const key = applySteps(board, move.steps, player).join(',');
    if (!nativeBoards.has(key)) nativeBoards.set(key, index);
  });
  const seen = new Set<string>();
  return legalCheckerTurns(board, player, dice).flatMap((turn) => {
    if (turn.dice[0] !== dice[0]) return [];
    const moveIndex = nativeBoards.get(turn.board.join(','));
    if (moveIndex === undefined) return [];
    const key = JSON.stringify(turn.steps);
    if (seen.has(key)) return [];
    seen.add(key);
    return [
      {
        steps: turn.steps,
        dice: turn.dice,
        moveIndex,
        notation: nativeMoves[moveIndex].notation,
      },
    ];
  });
}

interface CheckerTurn {
  board: number[];
  steps: CheckerStep[];
  dice: number[];
}

/** Enumerate complete legal checker turns in either die order. */
export function legalCheckerTurns(
  board: number[],
  player: number,
  dice: number[],
): CheckerTurn[] {
  if (
    board.length !== 26 ||
    (player !== 0 && player !== 1) ||
    dice.length !== 2 ||
    dice.some((d) => !Number.isInteger(d) || d < 1 || d > 6)
  )
    return [];
  const sign = player === 0 ? 1 : -1;
  const own = (b: number[]) => b.some((n) => n * sign > 0);
  function possible(b: number[], die: number): CheckerStep[] {
    const local = player === 0 ? b : b.map((_, p) => -b[25 - p]);
    const froms =
      local[25] > 0
        ? [25]
        : Array.from({ length: 24 }, (_, i) => i + 1).filter(
            (p) => local[p] > 0,
          );
    return froms.flatMap((from) => {
      const to = Math.max(0, from - die);
      if (to > 0 && local[to] < -1) return [];
      if (
        to === 0 &&
        (from > 6 ||
          local.slice(7, 26).some((n) => n > 0) ||
          (die > from && local.slice(from + 1, 7).some((n) => n > 0)))
      )
        return [];
      return [
        {
          from: player === 0 ? from : 25 - from,
          to: to === 0 ? (player === 0 ? -1 : 26) : player === 0 ? to : 25 - to,
        },
      ];
    });
  }
  function enumerate(order: number[]): CheckerTurn[] {
    const results: CheckerTurn[] = [];
    function walk(
      b: number[],
      remaining: number[],
      steps: CheckerStep[],
      used: number[],
    ) {
      if (!remaining.length || !own(b)) {
        results.push({ board: b, steps, dice: used });
        return;
      }
      const [die, ...rest] = remaining,
        moves = possible(b, die);
      if (!moves.length) walk(b, rest, steps, used);
      else
        for (const step of moves)
          walk(
            applySteps(b, [step], player),
            rest,
            [...steps, step],
            [...used, die],
          );
    }
    walk(board, order, [], []);
    return results;
  }
  const order = dice[0] === dice[1] ? Array(4).fill(dice[0]) : [...dice];
  const forward = enumerate(order),
    all =
      dice[0] === dice[1]
        ? forward
        : [...forward, ...enumerate([...order].reverse())];
  const maxUsed = Math.max(0, ...all.map((turn) => turn.dice.length));
  const maxDie =
    maxUsed === 1
      ? Math.max(
          ...all.filter((t) => t.dice.length === 1).map((t) => t.dice[0]),
        )
      : 0;
  return all.filter(
    (turn) =>
      turn.steps.length > 0 &&
      (!own(turn.board) ||
        (turn.dice.length === maxUsed &&
          (maxUsed !== 1 || turn.dice[0] === maxDie))),
  );
}
