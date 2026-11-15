import { request } from './api';
import type { components } from './generated-api';
import {
  decisionFilterQuery,
  type MatchDecisionFilter,
} from './match-statistics-api';

export type ReviewDecision = components['schemas']['ReviewDecision'];
export type ReviewDecisionPage = components['schemas']['ReviewDecisionPage'];

export function reviewDecisions(
  matchId: string,
  filter: MatchDecisionFilter,
  options: {
    analysisId?: string;
    after?: string | null;
    page?: number;
    limit?: number;
    signal: AbortSignal;
  },
) {
  const query = new URLSearchParams(decisionFilterQuery(filter));
  query.set('limit', String(options.limit ?? 50));
  if (options.analysisId) query.set('analysis_id', options.analysisId);
  if (options.page != null) query.set('page', String(options.page));
  if (options.after) query.set('after', options.after);
  return request<ReviewDecisionPage>(
    `/matches/${encodeURIComponent(matchId)}/review-decisions?${query}`,
    { signal: options.signal },
  );
}

export type ReviewEmptyReason = 'missing' | 'filtered' | 'clean';

export async function reviewEmptyReason(
  matchId: string,
  scope: Pick<MatchDecisionFilter, 'game_no' | 'errors_only'>,
  {
    analysisId,
    narrowed,
    signal,
  }: {
    analysisId?: string;
    narrowed: boolean;
    signal: AbortSignal;
  },
): Promise<ReviewEmptyReason> {
  // The empty numbered page already rules out errors when no selection filters are set.
  if (scope.errors_only && narrowed && (await hasEvaluation(true))) {
    return 'filtered';
  }
  if (!(await hasEvaluation(false))) return 'missing';
  return scope.errors_only ? 'clean' : 'filtered';

  async function hasEvaluation(errorsOnly: boolean) {
    let after: string | null = null;
    do {
      const page = await reviewDecisions(
        matchId,
        {
          game_no: scope.game_no,
          player: '',
          kind: '',
          errors_only: errorsOnly,
        },
        { analysisId, after, limit: errorsOnly ? 1 : 100, signal },
      );
      if (page.items.some((row) => row.evaluation_status === 'evaluated')) {
        return true;
      }
      after = page.next_cursor ?? null;
    } while (after);
    return false;
  }
}
