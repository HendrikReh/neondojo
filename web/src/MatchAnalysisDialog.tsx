import { useEffect, useEffectEvent, useState } from 'react';
import ActionButton from './shared/ui/ActionButton';
import AnalysisRemainingTime from './AnalysisRemainingTime';
import AnalysisTimeEstimate from './AnalysisTimeEstimate';
import LibraryDialog from './shared/ui/LibraryDialog';
import {
  MatchAnalysisFields,
  useMatchAnalysisOptions,
} from './MatchAnalysisFields';
import {
  cancelJob,
  isCancelledJob,
  request,
  terminal,
  type Job,
} from './shared/api/api';
import { useI18n } from './shared/i18n/i18n';
import type { LibraryMatch } from './library-api';
import { useLibraryText } from './library-messages';
import {
  matchAnalysisRunsPath,
  type MatchAnalysisRun,
} from './match-statistics-api';
import { useMatchAnalysis, type MatchAnalysisEngine } from './useMatchAnalysis';

export default function MatchAnalysisDialog({
  match,
  defaultEngine,
  defaultPlies,
  configured,
  analysisInstallationKey,
  trackedJobs,
  onAccepted,
  onShowResults,
  onClose,
}: {
  match: Pick<LibraryMatch, 'id' | 'display_name' | 'white' | 'black'>;
  defaultEngine: MatchAnalysisEngine;
  defaultPlies: number;
  configured: boolean;
  analysisInstallationKey?: string;
  trackedJobs: Readonly<Record<string, Job>>;
  onAccepted: (job: Job) => void;
  onShowResults: (job: Job) => void;
  onClose: () => void;
}) {
  const { t, formatError, formatPercent } = useI18n();
  const library = useLibraryText();
  const options = useMatchAnalysisOptions({
    defaultEngine,
    defaultPlies,
    configured,
    analysisInstallationKey,
  });
  const { engineId, plies, analysisConfigured, catalog } = options;
  const [submittedId, setSubmittedId] = useState<string>();
  const [cancelling, setCancelling] = useState<string>();
  const [cancelError, setCancelError] = useState<unknown>();
  const { job, error, submitting, run } = useMatchAnalysis(
    match.id,
    engineId,
    undefined,
    true,
    (accepted) => {
      setSubmittedId(accepted.id);
      onAccepted(accepted);
    },
  );
  // Use the dialog's live result when available and follow the exact submitted
  // job in the background tracker even if the selected engine or latest run changes.
  const completed =
    job?.id === submittedId && job?.status === 'completed'
      ? job
      : submittedId && trackedJobs[submittedId]?.status === 'completed'
        ? trackedJobs[submittedId]
        : undefined;
  const showCompleted = useEffectEvent(onShowResults);
  useEffect(() => {
    if (completed && !submitting) showCompleted(completed);
  }, [completed, submitting]);
  const active = job && !terminal(job);
  const [saved, setSaved] = useState<Job>();
  useEffect(() => {
    const controller = new AbortController();
    void request<MatchAnalysisRun[]>(matchAnalysisRunsPath(match.id), {
      signal: controller.signal,
    })
      .then((runs) => {
        if (!controller.signal.aborted) {
          setSaved(runs.find((run) => run.status === 'completed'));
        }
      })
      .catch(() => {});
    return () => controller.abort();
  }, [match.id]);
  const result = job?.status === 'completed' ? job : saved;
  const failure = error || catalog?.error;
  return (
    <LibraryDialog
      title={t('matchAnalysis.start')}
      native
      initialFocus="first-field"
      busy={submitting}
      onClose={onClose}
    >
      <div className="library-dialog-form match-analysis-dialog">
        <p>
          <strong>
            {match.display_name.trim() || `${match.white} – ${match.black}`}
          </strong>
        </p>
        <div className="match-analysis-options">
          <MatchAnalysisFields
            options={options}
            disabled={submitting || !configured}
          />
        </div>
        {active ? (
          <div className="match-analysis-progress" role="status">
            <strong>
              {t(
                job.status === 'queued'
                  ? 'matchAnalysis.queued'
                  : 'matchAnalysis.running',
              )}
            </strong>
            <p>
              {t('matchAnalysis.progressText', {
                completed: job.completed_games,
                total: job.total_games,
                plies: job.plies,
                progress: formatPercent(job.progress / 100),
              })}
            </p>
            <AnalysisRemainingTime key={job.id} job={job} />
            <ActionButton
              icon="cancel"
              label={t('analysis.cancelJob')}
              disabled={cancelling === job.id}
              onClick={() => {
                const id = job.id;
                setCancelling(id);
                setCancelError(undefined);
                // The job's event stream reports the cancelled state.
                cancelJob(id)
                  .catch(setCancelError)
                  .finally(() =>
                    setCancelling((current) =>
                      current === id ? undefined : current,
                    ),
                  );
              }}
            />
          </div>
        ) : (
          <AnalysisTimeEstimate
            matchId={match.id}
            scoped={false}
            engineId={engineId}
            plies={plies}
            configured={analysisConfigured}
            installationKey={analysisInstallationKey}
          />
        )}
        {job?.status === 'completed' && (
          <p role="status">{t('matchAnalysis.completed')}</p>
        )}
        {job && isCancelledJob(job) && (
          <p role="status">{t('matchAnalysis.cancelled')}</p>
        )}
        {!!cancelError && (
          <p className="error" role="alert">
            {formatError(cancelError)}
          </p>
        )}
        {job?.status === 'failed' && !isCancelledJob(job) && (
          <p className="error" role="alert">
            {t('matchAnalysis.failed')}{' '}
            {job.error && t('error.engine', { details: job.error })}
          </p>
        )}
        {!!failure && (
          <p className="error" role="alert">
            {formatError(failure)}
          </p>
        )}
      </div>
      <div className="library-dialog-actions">
        {result && (
          <ActionButton
            type="button"
            icon="analysis"
            className="primary"
            label={t('matchAnalysis.showResults')}
            disabled={submitting}
            onClick={() => onShowResults(result)}
          />
        )}
        <ActionButton
          type="button"
          icon="analysis"
          className={result ? undefined : 'primary'}
          label={t(
            submitting
              ? 'matchAnalysis.submitting'
              : job?.status === 'completed'
                ? 'matchAnalysis.again'
                : 'matchAnalysis.start',
          )}
          disabled={!analysisConfigured || !!active || submitting}
          onClick={() => void run(plies)}
        />
        <button type="button" disabled={submitting} onClick={onClose}>
          {library('close')}
        </button>
      </div>
    </LibraryDialog>
  );
}
