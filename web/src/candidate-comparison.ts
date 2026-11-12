import type { Position } from './api';
import type { SourceCandidate } from './source-analysis-api';
import { projectMove } from './move-preview';

export type CandidatePosition = Pick<
  Position,
  'board' | 'player' | 'dice' | 'notation'
>;
export type ComparisonCandidate = Pick<
  SourceCandidate,
  'rank' | 'notation' | 'equity' | 'equity_kind' | 'probabilities' | 'depth'
>;
type EquityValue = Pick<SourceCandidate, 'equity' | 'equity_kind'>;
const knownUnits = new Set([
  'cubeful',
  'cubeless',
  'normalized_cubeful',
  'mwc',
]);

function probability(value: unknown): number | null {
  return typeof value === 'number' && value >= 0 && value <= 1 ? value : null;
}

/** Source gammon probabilities include backgammons. Split each side for display. */
function splitOutcomes(
  total: number | null,
  gammonOrBetter: number | null,
  backgammon: number | null,
) {
  // Imported probabilities can be stored as float32, including the operands of 1 - win.
  const roundingTolerance = 1e-7;
  if (
    (total !== null &&
      gammonOrBetter !== null &&
      gammonOrBetter - total > roundingTolerance) ||
    (backgammon !== null &&
      ((gammonOrBetter !== null &&
        backgammon - gammonOrBetter > roundingTolerance) ||
        (total !== null && backgammon - total > roundingTolerance)))
  ) {
    return { total, single: null, gammon: null, backgammon: null };
  }
  return {
    total,
    single:
      total !== null && gammonOrBetter !== null
        ? Math.max(0, total - gammonOrBetter)
        : null,
    gammon:
      gammonOrBetter !== null && backgammon !== null
        ? Math.max(0, gammonOrBetter - backgammon)
        : null,
    backgammon,
  };
}

export function candidateOutcomes(values: unknown) {
  const fields: readonly unknown[] = Array.isArray(values)
    ? values
    : values && typeof values === 'object'
      ? [
          'win' in values ? values.win : undefined,
          'win_gammon' in values ? values.win_gammon : undefined,
          'win_backgammon' in values ? values.win_backgammon : undefined,
          'lose' in values ? values.lose : undefined,
          'lose_gammon' in values ? values.lose_gammon : undefined,
          'lose_backgammon' in values ? values.lose_backgammon : undefined,
        ]
      : [];
  const [win, winGammon, winBackgammon, loss, lossGammon, lossBackgammon] =
    fields.map(probability);
  const totalWin = win ?? null;
  // Some import formats omit the total loss while providing the total win.
  const totalLoss = loss ?? (totalWin === null ? null : 1 - totalWin);
  return {
    win: splitOutcomes(totalWin, winGammon ?? null, winBackgammon ?? null),
    loss: splitOutcomes(totalLoss, lossGammon ?? null, lossBackgammon ?? null),
  };
}

/** Display a difference within one source and unit, without assigning error severity. */
export function candidateLoss(
  candidate: EquityValue,
  best?: EquityValue,
): number | null {
  if (
    !best ||
    !knownUnits.has(best.equity_kind) ||
    candidate.equity_kind !== best.equity_kind ||
    typeof best.equity !== 'number' ||
    !Number.isFinite(best.equity) ||
    typeof candidate.equity !== 'number' ||
    !Number.isFinite(candidate.equity)
  )
    return null;
  const loss = best.equity - candidate.equity;
  return loss >= 0 ? loss : null;
}

/** Reuse the board preview rules to recognize alternative notation for the recorded move. */
export function isPlayedCandidate(
  notation: string,
  played?: string,
  position?: CandidatePosition,
): boolean {
  if (!played) return false;
  if (notation.trim() === played.trim()) return true;
  if (!position) return false;
  const candidate = projectMove(position, notation);
  const recorded = projectMove(position, played);
  return (
    candidate.ok &&
    recorded.ok &&
    candidate.board.every((count, index) => count === recorded.board[index])
  );
}
