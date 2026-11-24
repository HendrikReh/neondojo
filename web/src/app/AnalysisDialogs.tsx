import type { Job, Match } from '../shared/http/api';
import BulkMatchAnalysisDialog from '../workspaces/analysis/match/BulkMatchAnalysisDialog';
import MatchAnalysisDialog from '../workspaces/analysis/match/MatchAnalysisDialog';
import type { MatchAnalysisEngine } from '../workspaces/analysis/match/useMatchAnalysis';
import type { LibraryMatch } from '../workspaces/library/library-api';

/** The matches chosen for an analysis. */
export type AnalysisSelection =
  | {
      kind: 'single';
      match: Match & Pick<LibraryMatch, 'display_name'>;
    }
  | { kind: 'bulk'; matches: LibraryMatch[]; unavailableCount: number };

/** Starts the analysis of one match or of several. */
export default function AnalysisDialogs({
  selection,
  engine,
  plies,
  installationKey,
  trackedJobs,
  onAccepted,
  onBulkAccepted,
  onShowResults,
  onClose,
}: {
  selection: AnalysisSelection;
  engine: MatchAnalysisEngine;
  plies: number;
  installationKey: string;
  trackedJobs: Record<string, Job>;
  onAccepted: (jobs: Job[]) => void;
  /** The databases of the accepted jobs, by job. */
  onBulkAccepted: (
    databases: Record<string, string | null>,
    continuesBatch: boolean,
  ) => void;
  onShowResults: (match: Match, job: Job) => void;
  onClose: () => void;
}) {
  if (selection.kind === 'single')
    return (
      <MatchAnalysisDialog
        key={selection.match.id}
        match={selection.match}
        defaultEngine={engine}
        defaultPlies={plies}
        configured
        analysisInstallationKey={installationKey}
        trackedJobs={trackedJobs}
        onAccepted={(job) => onAccepted([job])}
        onShowResults={(job) => onShowResults(selection.match, job)}
        onClose={onClose}
      />
    );
  return (
    <BulkMatchAnalysisDialog
      matches={selection.matches}
      unavailableCount={selection.unavailableCount}
      defaultEngine={engine}
      defaultPlies={plies}
      configured
      analysisInstallationKey={installationKey}
      onAccepted={(jobs, continuesBatch) => {
        if (!jobs.length) return;
        const databaseByMatch = new Map(
          selection.matches.map((match) => [match.id, match.database_id]),
        );
        onBulkAccepted(
          Object.fromEntries(
            jobs.map((job) => [
              job.id,
              databaseByMatch.get(job.match_uuid ?? '') ?? null,
            ]),
          ),
          continuesBatch,
        );
        onAccepted(jobs);
      }}
      onClose={onClose}
    />
  );
}
