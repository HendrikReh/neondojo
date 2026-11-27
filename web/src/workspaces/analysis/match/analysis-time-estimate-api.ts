import { request } from '../../../shared/http/api';
import {
  isNonEmptyString as nonempty,
  isPositiveFinite as positive,
  isRecord as object,
  isSafeIntegerAtLeast as integer,
} from '../../../shared/http/guards';
import { isAnalysisDepth } from '../../../shared/engine/analysis-depth';

import type { components } from '../../../generated-api';

export type AnalysisTimeEstimateResult = Required<
  components['schemas']['AnalysisTimeEstimate']
>;

export async function loadAnalysisTimeEstimate(
  matchId: string,
  plies: number,
  engineId: string,
  gameNo: number | undefined,
  signal: AbortSignal,
) {
  const query = new URLSearchParams({
    plies: String(plies),
    engine_id: engineId,
  });
  if (gameNo !== undefined) query.set('game_no', String(gameNo));
  const result = await request<unknown>(
    `/matches/${encodeURIComponent(matchId)}/analysis/estimate?${query}`,
    { signal },
  );
  return parseEstimate(result);
}

// This forecast is optional. A malformed successful response must become an
// unavailable estimate before any metadata reaches date formatting or i18n.
function parseEstimate(value: unknown): AnalysisTimeEstimateResult {
  if (
    !object(value) ||
    !nonempty(value.match_id) ||
    !(value.game_no === null || integer(value.game_no, 1)) ||
    !nonempty(value.engine_id) ||
    !(value.engine_version === null || nonempty(value.engine_version)) ||
    !isAnalysisDepth(value.plies) ||
    !integer(value.position_count, 0) ||
    !(value.estimated_ms === null || positive(value.estimated_ms)) ||
    ![
      null,
      'no_matching_benchmark',
      'engine_unavailable',
      'no_checker_moves',
    ].includes(value.unavailable_reason as string | null)
  )
    throw new Error('Invalid analysis time estimate');

  if (value.unavailable_reason !== null) {
    if (
      value.estimated_ms !== null ||
      value.benchmark !== null ||
      (value.unavailable_reason === 'no_checker_moves' &&
        value.position_count !== 0)
    )
      throw new Error('Invalid unavailable estimate');
  } else {
    const benchmark = value.benchmark;
    if (
      value.position_count === 0 ||
      !positive(value.estimated_ms) ||
      !object(benchmark) ||
      !nonempty(benchmark.id) ||
      !nonempty(benchmark.finished_at) ||
      !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/.test(
        benchmark.finished_at,
      ) ||
      !Number.isFinite(Date.parse(benchmark.finished_at)) ||
      !nonempty(benchmark.reference_name) ||
      !positive(benchmark.average_move_ms) ||
      !nonempty(benchmark.protocol_id)
    )
      throw new Error('Invalid analysis benchmark basis');
  }
  return value as unknown as AnalysisTimeEstimateResult;
}
