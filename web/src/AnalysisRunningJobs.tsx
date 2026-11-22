import { useState } from 'react';
import ActionButton from './ActionButton';
import { cancelJob, jobEngine, type Job } from './api';
import { useI18n } from './i18n';
import { useStatisticsText } from './review-statistics-messages';

export default function AnalysisRunningJobs({
  jobs,
  displayedJobId,
}: {
  jobs: Job[];
  displayedJobId?: string;
}) {
  const { t, formatPercent, formatNumber, formatError } = useI18n();
  const statistics = useStatisticsText();
  const [cancelling, setCancelling] = useState<Set<string>>(new Set());
  const [cancelError, setCancelError] = useState<unknown>();
  const background = jobs.filter((job) => job.id !== displayedJobId);
  if (!background.length) return null;
  const cancel = (id: string) => {
    setCancelling((current) => new Set(current).add(id));
    setCancelError(undefined);
    // The job's event stream removes it from this list once it has stopped.
    cancelJob(id)
      .catch(setCancelError)
      .finally(() =>
        setCancelling((current) => {
          const next = new Set(current);
          next.delete(id);
          return next;
        }),
      );
  };
  return (
    <div className="analysis-running-jobs">
      {background.map((job) => {
        const scope =
          job.kind === 'match'
            ? job.game_no == null
              ? t('gameAnalysis.scopeMatch')
              : statistics('game', { game: job.game_no })
            : t(
                job.kind === 'dice'
                  ? 'analysis.diceEquity'
                  : 'analysis.candidates',
              );
        const context = t('analysis.jobContext', {
          scope,
          engine: jobEngine(job) === 'bgsage' ? 'NeonGammon' : 'GNUbg',
          plies: formatNumber(job.plies),
        });
        return (
          <div
            key={job.id}
            className="job-progress analysis-running-job"
            role="status"
          >
            <div>
              <strong>{context}</strong>
              <span>
                {job.status === 'queued'
                  ? t('analysis.jobQueued')
                  : t(
                      job.kind === 'dice'
                        ? 'analysis.diceRunning'
                        : 'analysis.movesRunning',
                    )}
                {' · '}
                {formatPercent(job.progress / 100)}
              </span>
            </div>
            <progress
              aria-label={t('analysis.progress')}
              max={100}
              value={job.progress}
            />
            <ActionButton
              icon="cancel"
              label={t('analysis.cancelJob')}
              compact
              disabled={cancelling.has(job.id)}
              onClick={() => cancel(job.id)}
            />
          </div>
        );
      })}
      {!!cancelError && <p role="alert">{formatError(cancelError)}</p>}
    </div>
  );
}
