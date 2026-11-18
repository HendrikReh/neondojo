import { jobEngine, type Job } from './api';
import { useI18n } from './i18n';
import { useStatisticsText } from './review-statistics-messages';

export default function AnalysisRunningJobs({
  jobs,
  displayedJobId,
}: {
  jobs: Job[];
  displayedJobId?: string;
}) {
  const { t, formatPercent, formatNumber } = useI18n();
  const statistics = useStatisticsText();
  const background = jobs.filter((job) => job.id !== displayedJobId);
  if (!background.length) return null;
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
          engine: jobEngine(job) === 'bgsage' ? 'Open Sage' : 'GNUbg',
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
          </div>
        );
      })}
    </div>
  );
}
