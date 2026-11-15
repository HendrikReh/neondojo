import { errorGrades } from './move-severity';
import { useEffect, useRef, useState } from 'react';
import { request, type Job } from './api';
import type { components } from './generated-api';
import { statisticsRevision } from './match-statistics-api';

export type MoveAssessment = components['schemas']['MoveAssessment'];
export type GameMoveAssessments = components['schemas']['GameMoveAssessments'];
export interface MoveAssessmentScope {
  matchId?: string;
  gameNo?: number;
  job?: Job;
  analysisId?: string;
  /** Completion/update identity of a newly available individual checker hint. */
  analysisRevision?: string;
}

export function assessmentIdentity({
  matchId,
  gameNo,
  job,
  analysisRevision,
  analysisId,
}: MoveAssessmentScope) {
  return `${matchId ?? ''}:${gameNo ?? ''}:${job ? `${statisticsRevision(job)}:${job.kind}:${job.match_uuid}:${job.game_no}` : ''}:${analysisId ?? ''}:${analysisRevision ?? ''}`;
}

export function assessmentRequestPath({
  matchId,
  gameNo,
  job,
  analysisId,
}: MoveAssessmentScope): string | undefined {
  if (
    !matchId ||
    gameNo === undefined ||
    !Number.isInteger(gameNo) ||
    gameNo <= 0
  )
    return undefined;
  if (
    job &&
    (job.kind !== 'match' ||
      job.match_uuid !== matchId ||
      (job.game_no != null && job.game_no !== gameNo))
  )
    return undefined;
  const query = new URLSearchParams({ game_no: String(gameNo) });
  if (analysisId) query.set('analysis_id', analysisId);
  else if (job) query.set('job_id', job.id);
  return `/matches/${encodeURIComponent(matchId)}/move-assessments?${query}`;
}

/** Fence server identity as well as the request's abort/selection identity. */
export function assessmentItems(
  scope: MoveAssessmentScope,
  data: GameMoveAssessments,
): Record<string, MoveAssessment> {
  if (
    data.match_id !== scope.matchId ||
    data.game_no !== scope.gameNo ||
    (scope.analysisId
      ? data.analysis_id !== scope.analysisId
      : data.job_id !== (scope.job?.id ?? null)) ||
    !Array.isArray(data.items)
  )
    return {};
  return Object.fromEntries(
    data.items
      .filter(
        (item) =>
          item &&
          typeof item.position_id === 'string' &&
          item.position_id.length > 0 &&
          (item.severity === 'none' || errorGrades.includes(item.severity)) &&
          Number.isFinite(item.equity_loss) &&
          item.equity_loss >= 0 &&
          ['native', 'imported'].includes(item.source) &&
          typeof item.engine === 'string' &&
          item.engine.trim().length > 0,
      )
      .map((item) => [item.position_id, item]),
  );
}

interface AssessmentState {
  identity: string;
  assessments: Record<string, MoveAssessment>;
  loading: boolean;
  error: unknown;
}

export function useMoveAssessments(scope: MoveAssessmentScope) {
  const identity = assessmentIdentity(scope);
  const path = assessmentRequestPath(scope);
  const current = useRef(identity);
  current.current = identity;
  const [state, setState] = useState<AssessmentState>({
    identity,
    assessments: {},
    loading: !!path,
    error: null,
  });
  useEffect(() => {
    const abort = new AbortController();
    setState({ identity, assessments: {}, loading: !!path, error: null });
    if (path) {
      void request<GameMoveAssessments>(path, { signal: abort.signal })
        .then((data) => {
          if (!abort.signal.aborted && current.current === identity)
            setState({
              identity,
              assessments: assessmentItems(scope, data),
              loading: false,
              error: null,
            });
        })
        .catch((error: unknown) => {
          if (!abort.signal.aborted && current.current === identity)
            setState({ identity, assessments: {}, loading: false, error });
        });
    }
    return () => abort.abort();
  }, [identity, path]);
  // Selection can change during render before its effect aborts the old request.
  // Never expose previous game/job colors in that intervening render.
  if (state.identity !== identity)
    return {
      assessments: {} as Record<string, MoveAssessment>,
      loading: !!path,
      error: null,
    };
  return {
    assessments: state.assessments,
    loading: state.loading,
    error: state.error,
  };
}
