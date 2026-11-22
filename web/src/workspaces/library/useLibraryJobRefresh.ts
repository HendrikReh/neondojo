import { useEffect, useRef } from 'react';
import { request } from '../../shared/http/api';
import { watchJobChanges } from '../../shared/jobs/job-watch';
import {
  matchAnalysisRunsPath,
  type MatchAnalysisRun,
} from '../analysis/match/match-statistics-api';
import type { LibraryMatch } from './library-api';

/** Page reloads while the jobs behind active rows cannot be read. */
export const LIBRARY_FALLBACK_RELOAD_MS = 15_000;

const pending = (status: string | null | undefined) =>
  status === 'queued' || status === 'running';

/**
 * Reload the shown library page when a queued or running whole-match analysis
 * in it changes state. The shared job watch follows those jobs, so the table is
 * read again only on a transition, or slowly while their jobs cannot be read.
 */
export function useLibraryJobRefresh(
  matches: readonly LibraryMatch[],
  visible: boolean,
  reload: () => void,
) {
  // Position results can show one match in several rows.
  const active = visible
    ? JSON.stringify(
        [
          ...new Map(
            matches
              .filter((match) => pending(match.analysis_status))
              .map((match) => [match.id, match.analysis_status]),
          ),
        ].sort(),
      )
    : '[]';
  // Rows whose shown status is unchanged keep their jobs across reloads, so a
  // transition in one row does not read the jobs of every other row again.
  const resolved = useRef(new Map<string, MatchAnalysisRun[]>());
  useEffect(() => {
    const rows = JSON.parse(active) as [string, string][];
    const keys = new Set(rows.map((row) => row.join(':')));
    for (const key of resolved.current.keys())
      if (!keys.has(key)) resolved.current.delete(key);
    if (!rows.length) return;
    const abort = new AbortController();
    const stops: (() => void)[] = [];
    let fallback: ReturnType<typeof setInterval> | undefined;
    const changed = () => {
      if (!abort.signal.aborted) reload();
    };
    function follow(key: string, shown: string, jobs: MatchAnalysisRun[]) {
      const status = jobs.some((job) => job.status === 'running')
        ? 'running'
        : 'queued';
      // The row is already out of date.
      if (!jobs.length || status !== shown) return changed();
      stops.push(
        watchJobChanges(jobs, (job) => {
          // A finished job no longer describes the row.
          if (!job || !pending(job.status)) resolved.current.delete(key);
          // Only the status is shown in the table.
          if (!job || !pending(job.status) || job.status !== status) changed();
        }),
      );
    }
    for (const [matchId, shown] of rows) {
      const key = `${matchId}:${shown}`;
      const known = resolved.current.get(key);
      if (known) {
        follow(key, shown, known);
        continue;
      }
      request<MatchAnalysisRun[]>(matchAnalysisRunsPath(matchId), {
        signal: abort.signal,
      })
        .then((runs) => {
          if (abort.signal.aborted) return;
          const jobs = runs.filter(
            (run) =>
              run.kind === 'match' &&
              run.game_no == null &&
              pending(run.status),
          );
          resolved.current.set(key, jobs);
          follow(key, shown, jobs);
        })
        .catch(() => {
          if (abort.signal.aborted || fallback !== undefined) return;
          fallback = setInterval(changed, LIBRARY_FALLBACK_RELOAD_MS);
        });
    }
    return () => {
      abort.abort();
      stops.forEach((stop) => stop());
      clearInterval(fallback);
    };
  }, [active, reload]);
}
