import type { Job, MatchAnalysisSummary } from './shared/api/api';

import type { components } from './generated-api';
import { errorGrades } from './shared/review/move-severity';

type Schema = components['schemas'];
export type DecisionKind = 'checker' | 'cube';
export type PlayerErrorBreakdown = Schema['PlayerErrorBreakdown'];
export type GameAnalysisStatistics = Schema['GameAnalysisStatistics'];
export type JobAnalysisStatistics = Schema['JobAnalysisStatistics'];
export type MatchAnalysisRun = Schema['AnalysisComparisonRun'];

export function matchAnalysisRunsPath(matchId: string, gameNo?: number) {
  return `/matches/${matchId}/analysis/jobs${gameNo === undefined ? '' : `?game_no=${gameNo}`}`;
}

export interface MatchDecisionFilter {
  player: string;
  kind: string;
  severity?: string;
  game_no?: number;
  errors_only: boolean;
}
export type StatisticsMetric =
  | 'decisions'
  | 'checker'
  | 'cube'
  | 'loss'
  | 'inaccuracies'
  | (typeof errorGrades)[number]
  | 'pr';
export interface StatisticsAction {
  metric: StatisticsMetric;
  kind?: DecisionKind;
  player: number;
  gameNo?: number;
}
/** A metric replaces the selection, so an old severity or game cannot hide it. */
export function buildStatisticsFilter(
  action: StatisticsAction,
): MatchDecisionFilter {
  const severity = errorGrades.find((grade) => grade === action.metric) ?? '';
  return {
    player: String(action.player),
    kind:
      action.metric === 'checker' || action.metric === 'cube'
        ? action.metric
        : (action.kind ?? ''),
    severity,
    game_no: action.gameNo,
    errors_only: !!severity || action.metric === 'inaccuracies',
  };
}
export function decisionFilterQuery(filter: MatchDecisionFilter) {
  const query = new URLSearchParams({
    errors_only: String(filter.errors_only),
  });
  if (filter.player) query.set('player', filter.player);
  if (filter.kind) query.set('kind', filter.kind);
  if (filter.severity) query.set('severity', filter.severity);
  if (filter.game_no !== undefined)
    query.set('game_no', String(filter.game_no));
  return query.toString();
}
export function statisticsRevision(job: Job) {
  return `${job.id}:${job.completed_games}:${job.status}`;
}
/** Only display a supplied, current native PR; never derive one from move losses. */
export function nativePerformanceRating(
  summary:
    MatchAnalysisSummary | components['schemas']['CatalogAnalysisSummary'],
  player: number,
): number | undefined {
  const rating = summary.performance_rating;
  if (
    summary.plies == null ||
    !rating ||
    rating.method !== 'gnubg_emg_500_v1' ||
    rating.profile !==
      `${summary.engine}; ${summary.plies}-ply cubeful deterministic` ||
    rating.games <= 0 ||
    rating.games !== summary.completed_games
  )
    return undefined;
  const stats = rating.players.find((entry) => entry.player === player);
  if (
    !stats ||
    stats.value == null ||
    !Number.isFinite(stats.value) ||
    stats.value < 0 ||
    !Number.isFinite(stats.normalized_equity_loss) ||
    stats.normalized_equity_loss < 0 ||
    stats.unforced_checker_moves < 0 ||
    stats.close_cube_decisions < 0
  )
    return undefined;
  const denominator = stats.unforced_checker_moves + stats.close_cube_decisions;
  if (
    denominator <= 0 ||
    Math.abs(stats.value - (500 * stats.normalized_equity_loss) / denominator) >
      1e-9
  )
    return undefined;
  return stats.value;
}
