import { useEffect } from 'react';
import { terminal, type Job } from '../shared/http/api';
import type { decodeBulkAnalysisSession } from './workspace-state';

export type BulkAnalysisSession = NonNullable<
  ReturnType<typeof decodeBulkAnalysisSession>
>;

export function bulkJobPending(
  id: string,
  tracked: Record<string, Job>,
  missing: ReadonlySet<string>,
) {
  const job = tracked[id];
  return !missing.has(id) && (!job || !terminal(job));
}

/**
 * The progress of the bulk analysis in one database. While it runs, a reload
 * from the match list returns there.
 */
export function useBulkAnalysis({
  session,
  setSession,
  databaseId,
  tracked,
  missing,
  listVisible,
}: {
  session: BulkAnalysisSession;
  setSession: (
    update: (previous: BulkAnalysisSession) => BulkAnalysisSession,
  ) => void;
  databaseId: string;
  tracked: Record<string, Job>;
  missing: ReadonlySet<string>;
  listVisible: boolean;
}) {
  const bulkJobIds = Object.keys(session.jobs);
  const visibleBulkJobIds = databaseId
    ? bulkJobIds.filter((id) => session.jobs[id] === databaseId)
    : bulkJobIds;
  const progress = {
    total: visibleBulkJobIds.length,
    completed: 0,
    failed: 0,
    missing: 0,
    loading: false,
  };
  for (const id of visibleBulkJobIds) {
    const status = tracked[id]?.status;
    if (missing.has(id)) progress.missing++;
    else if (status === 'completed') progress.completed++;
    else if (status === 'failed') progress.failed++;
    else if (!status) progress.loading = true;
  }
  const bulkAnalysisInProgress = bulkJobIds.some((id) =>
    bulkJobPending(id, tracked, missing),
  );
  const reopenMatchList = listVisible && bulkAnalysisInProgress;
  useEffect(() => {
    if (session.showMatchList !== reopenMatchList)
      setSession((previous) => ({
        ...previous,
        showMatchList: reopenMatchList,
      }));
  }, [session.showMatchList, reopenMatchList, setSession]);
  return progress;
}
