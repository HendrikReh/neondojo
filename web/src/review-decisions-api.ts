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
  else if (options.jobId) query.set('job_id', options.jobId);
  if (options.page != null) query.set('page', String(options.page));
  if (options.after) query.set('after', options.after);
  return request<ReviewDecisionPage>(
    `/matches/${encodeURIComponent(matchId)}/review-decisions?${query}`,
    { signal: options.signal },
  );
}

export function canOpenReviewDecision(row: ReviewDecision) {
  return row.kind === 'cube' ? !!row.review_position : !!row.position_uuid;
}

/** The saved review row of one recorded cube action, if the analysis has it. */
export async function findCubeReviewDecision(
  matchId: string,
  gameNo: number,
  moveId: string,
  options: { analysisId: string; signal: AbortSignal },
): Promise<ReviewDecision | undefined> {
  let after: string | null = null;
  do {
    const page = await reviewDecisions(
      matchId,
      { game_no: gameNo, player: '', kind: 'cube', errors_only: false },
      { ...options, after, limit: 100 },
    );
    const row = page.items.find((item) => item.move_id === moveId);
    if (row) return row;
    after = page.next_cursor ?? null;
  } while (after);
  return undefined;
}

/** The saved cube decision with this number in the game's decision sequence. */
export async function findCubeDecisionByNumber(
  matchId: string,
  gameNo: number,
  decisionNo: number,
  options: { analysisId: string; signal: AbortSignal },
): Promise<ReviewDecision | undefined> {
  let after: string | null = null;
  do {
    const page = await reviewDecisions(
      matchId,
      { game_no: gameNo, player: '', kind: 'cube', errors_only: false },
      { ...options, after, limit: 100 },
    );
    const row = page.items.find((item) => item.decision_no === decisionNo);
    if (row) return row;
    after = page.next_cursor ?? null;
  } while (after);
  return undefined;
}

export type ReviewEmptyReason = 'missing' | 'filtered' | 'clean';

export async function reviewEmptyReason(
  matchId: string,
  scope: Pick<MatchDecisionFilter, 'game_no' | 'errors_only' | 'kind'>,
  {
    jobId,
    analysisId,
    narrowed,
    signal,
  }: {
    jobId?: string;
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
          kind: scope.kind,
          errors_only: errorsOnly,
        },
        { jobId, analysisId, after, limit: errorsOnly ? 1 : 100, signal },
      );
      if (page.items.some((row) => row.evaluation_status === 'evaluated')) {
        return true;
      }
      after = page.next_cursor ?? null;
    } while (after);
    return false;
  }
}

export type ReviewNavigationOrigin = {
  game?: number;
  ply?: number;
  row?: ReviewDecision;
};

export type ReviewListEntry = { row: ReviewDecision; ordinal: number };

type ReviewPageNavigation = {
  page: number;
  pageSize: number;
  total: number;
};

export async function findReviewNeighbor({
  matchId,
  filter,
  jobId,
  analysisId,
  origin,
  direction,
  signal,
  currentPage,
}: {
  matchId: string;
  filter: MatchDecisionFilter;
  jobId?: string;
  analysisId?: string;
  origin: ReviewNavigationOrigin;
  direction: 'previous' | 'next';
  signal: AbortSignal;
  currentPage?: ReviewPageNavigation;
}): Promise<ReviewListEntry | undefined> {
  if (currentPage) {
    const { page, pageSize, total } = currentPage;
    const lastPage = Math.ceil(total / pageSize);
    if (!lastPage) return undefined;
    const step = direction === 'next' ? 1 : -1;
    const wrap = (number: number) => ((number - 1 + lastPage) % lastPage) + 1;
    for (let offset = 1; offset <= lastPage; offset += 1) {
      const target = wrap(page + step * offset);
      const result = await reviewDecisions(matchId, filter, {
        jobId,
        analysisId,
        page: target,
        limit: pageSize,
        signal,
      });
      const rows =
        direction === 'next' ? result.items : [...result.items].reverse();
      const index = rows.findIndex(canOpenReviewDecision);
      if (index >= 0) {
        const inPage = direction === 'next' ? index : rows.length - index - 1;
        return {
          row: rows[index],
          ordinal: (target - 1) * pageSize + inPage + 1,
        };
      }
    }
    return undefined;
  }

  // The review cursor orders the whole filtered list, including rows without
  // a board position. Keep their places in the count and skip them on jumps.
  let ordinal = 0;
  let first: ReviewListEntry | undefined;
  let last: ReviewListEntry | undefined;
  let previous: ReviewListEntry | undefined;
  let after: string | null = null;
  do {
    const page = await reviewDecisions(matchId, filter, {
      jobId,
      analysisId,
      after,
      limit: 100,
      signal,
    });
    for (const row of page.items) {
      ordinal += 1;
      if (!canOpenReviewDecision(row)) continue;
      const entry = { row, ordinal };
      const relation = compareReviewPosition(row, origin);
      if (relation > 0) {
        if (direction === 'next') return entry;
        if (previous) return previous;
      }
      first ??= entry;
      last = entry;
      if (relation < 0) previous = entry;
    }
    after = page.next_cursor ?? null;
  } while (after);
  return direction === 'next' ? first : (previous ?? last);
}

function compareReviewPosition(
  row: ReviewDecision,
  origin: ReviewNavigationOrigin,
) {
  const anchor = origin.row;
  const game = anchor?.game_no ?? origin.game ?? 0;
  if (row.game_no !== game) return row.game_no - game;
  if (anchor?.id === row.id) return 0;
  if (anchor && anchor.source === row.source) {
    return (
      row.decision_no - anchor.decision_no ||
      Number(row.kind === 'checker') - Number(anchor.kind === 'checker') ||
      (row.id < anchor.id ? -1 : 1)
    );
  }
  const checkerStep = row.checker_no ?? row.next_checker_no ?? Infinity;
  const ply = origin.ply ?? 0;
  return checkerStep === ply && row.kind === 'cube' ? -1 : checkerStep - ply;
}
