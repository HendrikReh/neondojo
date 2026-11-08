import { useState } from 'react';
import { type Analysis, type Distribution, type DiceStats } from './api';
import type { Jobs } from './useJobs';
import { useI18n } from './i18n';

export default function AnalysisPanel({
  jobs,
  stats,
  disabled,
  onAnalyze,
}: {
  jobs: Jobs;
  stats: DiceStats | null;
  disabled: boolean;
  onAnalyze: (kind: 'analysis' | 'dice', plies: number) => void;
}) {
  const { t, formatNumber, formatPercent, formatEquity, formatDecision } =
    useI18n();
  const [plies, setPlies] = useState(0);
  const [view, setView] = useState<'analysis' | 'dice' | 'observed'>(
    'analysis',
  );
  const [selectedRoll, setSelectedRoll] = useState('1-1');
  const job =
    Object.values(jobs).find(
      (j) => j.status === 'queued' || j.status === 'running',
    ) || jobs[view === 'dice' ? 'dice' : 'analysis'];
  const pending =
    !!job && (job.status === 'queued' || job.status === 'running');
  const analysis =
    jobs.analysis?.status === 'completed'
      ? (jobs.analysis.result as Analysis)
      : null;
  const distribution =
    jobs.dice?.status === 'completed'
      ? (jobs.dice.result as Distribution)
      : null;
  const choose = (kind: 'analysis' | 'dice') => {
    setView(kind);
    onAnalyze(kind, plies);
  };
  const best = distribution?.rolls.find(
    (r) => r.dice.join('-') === selectedRoll,
  );
  return (
    <section className="analysis-panel">
      <div className="section-heading">
        <div>
          <span className="eyebrow">{t('analysis.eyebrow')}</span>
          <h2>{t('analysis.title')}</h2>
        </div>
        <span className="engine-label">GNUbg · {t('common.local')}</span>
      </div>
      <div className="analysis-controls">
        <label className="depth-label">
          {t('analysis.depth')}{' '}
          <select
            aria-label={t('analysis.depth')}
            value={plies}
            onChange={(e) => setPlies(Number(e.target.value))}
          >
            <option value={0}>{t('analysis.fast')}</option>
            <option value={1}>1-ply</option>
            <option value={2}>{t('analysis.accurate')}</option>
          </select>
        </label>
        <button
          className="primary"
          disabled={disabled || pending}
          onClick={() => choose('analysis')}
        >
          {t('analysis.analyze')} <span aria-hidden="true">↗</span>
        </button>
        <button disabled={disabled || pending} onClick={() => choose('dice')}>
          {t('analysis.distribution')}
        </button>
      </div>
      {pending && (
        <div className="job-progress" role="status">
          <div>
            <span>
              {job.status === 'queued'
                ? t('analysis.queued')
                : job.kind === 'dice'
                  ? t('analysis.diceRunning')
                  : t('analysis.movesRunning')}
            </span>
            <b>{formatPercent(job.progress / 100)}</b>
          </div>
          <progress
            aria-label={t('analysis.progress')}
            max="100"
            value={job.progress}
          />
        </div>
      )}
      {job?.status === 'failed' && (
        <div className="error" role="alert">
          <strong>{t('analysis.failed')}</strong>{' '}
          {job.error && t('error.engine', { details: job.error })}{' '}
          {t('analysis.retryHint')}
        </div>
      )}
      <div className="tabs" role="tablist" aria-label={t('analysis.views')}>
        <button
          role="tab"
          aria-selected={view === 'analysis'}
          onClick={() => setView('analysis')}
        >
          {t('analysis.candidates')}
        </button>
        <button
          role="tab"
          aria-selected={view === 'dice'}
          onClick={() => setView('dice')}
        >
          {t('analysis.diceEquity')}
        </button>
        <button
          role="tab"
          aria-selected={view === 'observed'}
          onClick={() => setView('observed')}
        >
          {t('analysis.observed')}
        </button>
      </div>
      {view === 'analysis' &&
        (analysis ? (
          <>
            <table className="candidates">
              <thead>
                <tr>
                  <th>{t('analysis.rank')}</th>
                  <th>{t('common.move')}</th>
                  <th>{t('common.equity')}</th>
                  <th>{t('analysis.win')}</th>
                </tr>
              </thead>
              <tbody>
                {analysis.candidates.map((c) => (
                  <tr key={c.rank} className={c.rank === 1 ? 'best-move' : ''}>
                    <td>{formatNumber(c.rank, { minimumIntegerDigits: 2 })}</td>
                    <td>
                      {formatDecision(c.notation)}
                      {c.rank === 1 && (
                        <span className="best-label">
                          {t('analysis.bestMove')}
                        </span>
                      )}
                    </td>
                    <td>{formatEquity(c.equity)}</td>
                    <td>{formatPercent(c.probabilities[0], 1)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="fineprint">
              {t('analysis.method', {
                engine: analysis.engine,
                plies: analysis.plies,
              })}
            </p>
          </>
        ) : (
          <div className="panel-empty">
            <span className="empty-symbol">↗</span>
            <h3>{t('analysis.emptyHeading')}</h3>
            <p>{t('analysis.emptyDescription')}</p>
          </div>
        ))}
      {view === 'dice' &&
        (distribution ? (
          <div className="distribution">
            <div className="distribution-caption">
              <span>{t('analysis.combinations')}</span>
              <strong>Ø {formatEquity(distribution.mean_equity)}</strong>
            </div>
            <div className="dice-grid">
              {Array.from({ length: 36 }, (_, i) => {
                const a = Math.floor(i / 6) + 1,
                  b = (i % 6) + 1;
                const roll = distribution.rolls.find(
                  (r) =>
                    r.dice[0] === Math.min(a, b) &&
                    r.dice[1] === Math.max(a, b),
                );
                return (
                  <button
                    key={i}
                    title={t('analysis.rollTitle', {
                      first: a,
                      second: b,
                      move: roll ? formatDecision(roll.best.notation) : '—',
                    })}
                    className={`${roll && roll.best.equity >= distribution.mean_equity ? 'above' : 'below'} ${selectedRoll === [Math.min(a, b), Math.max(a, b)].join('-') ? 'selected' : ''}`}
                    onClick={() =>
                      setSelectedRoll(
                        [Math.min(a, b), Math.max(a, b)].join('-'),
                      )
                    }
                  >
                    <span>
                      {a} · {b}
                    </span>
                    <b>{roll ? formatEquity(roll.best.equity) : '—'}</b>
                  </button>
                );
              })}
            </div>
            {best && (
              <p className="roll-detail">
                <strong>{best.dice.join('–')}</strong>
                <span>{formatDecision(best.best.notation)}</span>
                <b>{formatEquity(best.best.equity)}</b>
                <small>
                  {t('analysis.probability', {
                    value: formatPercent(best.probability, 2),
                  })}
                </small>
              </p>
            )}
            <p className="fineprint">
              {t('analysis.distributionMethod', { plies: distribution.plies })}
            </p>
          </div>
        ) : (
          <div className="panel-empty">
            <span className="empty-symbol">⚄</span>
            <h3>{t('analysis.diceEmptyHeading')}</h3>
            <p>{t('analysis.diceEmptyDescription')}</p>
          </div>
        ))}
      {view === 'observed' &&
        (stats ? (
          <div className="observed">
            <p className="fineprint">
              {t('analysis.observedMethod', { count: stats.total })}
            </p>
            <div className="roll-counts">
              {stats.rolls.map((r) => (
                <div key={r.dice.join('-')}>
                  <span>{r.dice.join('–')}</span>
                  <meter
                    aria-label={r.dice.join('–')}
                    value={r.count}
                    max={Math.max(1, ...stats.rolls.map((r) => r.count))}
                  />
                  <b>{formatNumber(r.count)}</b>
                </div>
              ))}
            </div>
          </div>
        ) : (
          <div className="panel-empty">
            <p>{t('analysis.selectMatch')}</p>
          </div>
        ))}
    </section>
  );
}
