import type { Candidate, Position } from './shared/api/api';
import type { PositionAnalysisResult } from './analysis-catalog-api';
import type { SourceCandidate } from './source-analysis-api';
import { projectMove } from './shared/board/move-preview';

export type CandidatePosition = Pick<
  Position,
  'board' | 'player' | 'dice' | 'notation'
>;
export type ComparisonCandidate = Pick<
  SourceCandidate,
  'rank' | 'notation' | 'equity' | 'equity_kind' | 'probabilities' | 'depth'
> &
  Pick<Candidate, 'path'>;
type EquityValue = Pick<SourceCandidate, 'equity' | 'equity_kind'>;
const knownUnits = new Set([
  'cubeful',
  'cubeless',
  'normalized_cubeful',
  'mwc',
]);

/**
 * Whether a candidate's depth only repeats the depth recorded for its table.
 * A table may qualify the same search depth, as in "2-ply cubeful".
 */
export function repeatsTableDepth(
  candidateDepth?: string | null,
  tableDepth?: string | null,
) {
  if ((candidateDepth ?? null) === (tableDepth ?? null)) return true;
  return (
    !!candidateDepth &&
    /^\d+-ply$/.test(candidateDepth) &&
    !!tableDepth?.startsWith(`${candidateDepth} `)
  );
}

export function candidateEquityDifference(
  candidate?: EquityValue,
  reference?: EquityValue,
): number | null {
  const unit = (kind: string) =>
    kind === 'normalized_cubeful' ? 'cubeful' : kind;
  if (
    typeof candidate?.equity !== 'number' ||
    typeof reference?.equity !== 'number' ||
    !knownUnits.has(reference.equity_kind) ||
    unit(candidate.equity_kind) !== unit(reference.equity_kind)
  )
    return null;
  const difference = candidate.equity - reference.equity;
  return Number.isFinite(difference) ? difference : null;
}

export function nativeCheckerCandidates(
  result?: PositionAnalysisResult | null,
) {
  const candidates =
    result?.analysis?.candidates ?? result?.decision?.checker_candidates ?? [];
  return candidates.map((candidate) => ({
    ...candidate,
    equity_kind: result?.decision ? 'normalized_cubeful' : 'cubeful',
    depth: candidate.plies == null ? null : `${candidate.plies}-ply`,
  }));
}

export function candidateEquityRanks(candidates: ComparisonCandidate[]) {
  const unit = candidates[0]?.equity_kind;
  if (!unit || !knownUnits.has(unit))
    return new Map<ComparisonCandidate, number>();
  const evaluated = candidates.flatMap((candidate) =>
    candidate.equity_kind === unit &&
    typeof candidate.equity === 'number' &&
    Number.isFinite(candidate.equity)
      ? [{ candidate, equity: candidate.equity }]
      : [],
  );
  if (evaluated.length !== candidates.length)
    return new Map<ComparisonCandidate, number>();
  return new Map(
    evaluated
      .sort(
        (a, b) => b.equity - a.equity || a.candidate.rank - b.candidate.rank,
      )
      .map(({ candidate }, index) => [candidate, index + 1]),
  );
}

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
  if (candidate.equity_kind !== best?.equity_kind) return null;
  const loss = candidateEquityDifference(best, candidate);
  return loss !== null && loss >= 0 ? loss : null;
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
