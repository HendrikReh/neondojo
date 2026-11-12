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
    jobId?: string;
    after?: string | null;
    limit?: number;
    signal: AbortSignal;
  },
) {
  const query = new URLSearchParams(decisionFilterQuery(filter));
  query.set('limit', String(options.limit ?? 50));
  if (options.jobId) query.set('job_id', options.jobId);
  if (options.after) query.set('after', options.after);
  return request<ReviewDecisionPage>(
    `/matches/${encodeURIComponent(matchId)}/review-decisions?${query}`,
    { signal: options.signal },
  );
}
