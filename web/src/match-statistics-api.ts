import type { Job, MatchAnalysisSummary } from './api';

import type { components } from './generated-api';

type Schema = components['schemas'];
export type GameAnalysisStatistics = Schema['GameAnalysisStatistics'];
export type JobAnalysisStatistics = Schema['JobAnalysisStatistics'];
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
  | 'doubtful'
  | 'bad'
  | 'very_bad'
  | 'pr';
export interface StatisticsAction {
  metric: StatisticsMetric;
  player: number;
  gameNo?: number;
}
/** A metric replaces the selection, so an old severity or game cannot hide it. */
export function buildStatisticsFilter(
  action: StatisticsAction,
): MatchDecisionFilter {
  const severity = ['doubtful', 'bad', 'very_bad'].includes(action.metric)
    ? action.metric
    : '';
  return {
    player: String(action.player),
    kind:
      action.metric === 'checker' || action.metric === 'cube'
        ? action.metric
        : '',
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
  summary: MatchAnalysisSummary,
  player: number,
): number | undefined {
  const rating = summary.performance_rating;
  if (
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
