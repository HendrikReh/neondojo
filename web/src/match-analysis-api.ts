import { request, type Job } from './shared/api/api';
import type { components } from './generated-api';

export type MatchAnalysisEngine = components['schemas']['MatchAnalysisEngine'];

export function submitMatchAnalysis(
  matchId: string,
  engineId: MatchAnalysisEngine,
  plies: number,
  gameNo?: number,
) {
  // Submission uses explicit options independently of profile writes.
  return request<Job>(`/matches/${matchId}/analysis`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      engine_id: engineId,
      plies,
      game_no: gameNo,
    } satisfies components['schemas']['MatchAnalysisRequest']),
  });
}
