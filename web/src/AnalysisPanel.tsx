import { useOptionalConfiguration } from './ConfigurationProvider';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import Tooltip from './Tooltip';
import ActionButton from './ActionButton';
import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import {
  type Analysis,
  type Distribution,
  type DiceStats,
  type Position,
} from './api';
import type { Jobs } from './useJobs';
import { useI18n } from './i18n';
import { useAnalysisSources } from './useAnalysisSources';
import { useSourceText } from './analysis-source-messages';
import SourceAnalysisView from './SourceAnalysisView';
import { type PreviewCandidate } from './candidate-preview';
import CandidateTable from './CandidateTable';
import './analysis-sources.css';
import { DEFAULT_ANALYSIS_PLIES } from './analysis-depth';

export default function AnalysisPanel({
  jobs,
  stats,
  gameNo,
  disabled,
  active = true,
  restoring = false,
  onAnalyze,
  defaultPlies = DEFAULT_ANALYSIS_PLIES,
  positionId,
  sourceRevision = 0,
  onPreview,
  selectedPreview,
  position,
  playerName,
  preferredSource,
}: {
  jobs: Jobs;
  stats: DiceStats | null;
  gameNo?: number;
  disabled: boolean;
  active?: boolean;
  restoring?: boolean;
  onAnalyze: (kind: 'analysis' | 'dice', plies: number) => void;
  defaultPlies?: number;
  positionId?: string;
  sourceRevision?: number;
  onPreview?: (candidate: PreviewCandidate | null) => void;
  selectedPreview?: PreviewCandidate;
  position?: Position;
  playerName?: string;
  preferredSource?: {
    positionId: string;
    source: 'file' | 'engine';
    token: number;
  };
}) {
  const {
    t,
    formatNumber,
    formatPercent,
    formatEquity,
    formatDecision,
    formatError,
  } = useI18n();
  const configuredEngine =
    useOptionalConfiguration()?.current?.analysis.engine ?? 'gnubg';
  const engineName = configuredEngine === 'bgsage' ? 'Open Sage' : 'GNUbg';
  const supportedPlies =
    configurationOptions.engine_profiles[configuredEngine].analysis;
  const l = useSourceText();
  const sources = useAnalysisSources(positionId, sourceRevision);
  const [sourceChoice, setSourceChoice] = useState<{
    positionId?: string;
    source: 'file' | 'book' | 'engine';
  }>();
  useEffect(() => {
    if (preferredSource && preferredSource.positionId === positionId) {
      setSourceChoice({ positionId, source: preferredSource.source });
      onPreview?.(null);
    }
  }, [preferredSource, positionId, onPreview]);
  const selectedSource =
    sourceChoice && sourceChoice.positionId === positionId
      ? sourceChoice.source
      : jobs.analysis?.status === 'completed'
        ? 'engine'
        : sources.records.length
          ? 'file'
          : sources.book?.hit
            ? 'book'
            : 'engine';
  useEffect(() => onPreview?.(null), [selectedSource, positionId, onPreview]);
  type Source = 'file' | 'book' | 'engine';
  function selectSource(source: Source) {
    onPreview?.(null);
    setSourceChoice({ positionId, source });
  }
  const sourceRefs = useRef<Partial<Record<Source, HTMLButtonElement | null>>>(
    {},
  );
  function sourceKey(event: KeyboardEvent<HTMLButtonElement>, current: Source) {
    const options: Source[] = sources.records.length
      ? ['file', 'book', 'engine']
      : ['book', 'engine'];
    const index = options.indexOf(current);
    let next: number;
    if (event.key === 'ArrowRight') next = (index + 1) % options.length;
    else if (event.key === 'ArrowLeft')
      next = (index + options.length - 1) % options.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = options.length - 1;
    else return;
    event.preventDefault();
    selectSource(options[next]);
    sourceRefs.current[options[next]]?.focus();
  }
  const [plies, setPlies] = useState(defaultPlies);
  useEffect(() => setPlies(defaultPlies), [defaultPlies, configuredEngine]);
  const [view, setView] = useState<'analysis' | 'dice' | 'observed'>(
    'analysis',
  );
  const [selectedRoll, setSelectedRoll] = useState('1-1');
  const diceJob = jobs.dice?.plies === plies ? jobs.dice : undefined;
  const job =
    Object.values(jobs).find(
      (j) => j.status === 'queued' || j.status === 'running',
    ) || (view === 'dice' ? diceJob : jobs.analysis);
  const pending =
    !!job && (job.status === 'queued' || job.status === 'running');
  const analysis =
    jobs.analysis?.status === 'completed'
      ? (jobs.analysis.result as Analysis)
      : null;
  const distribution =
    diceJob?.status === 'completed' ? (diceJob.result as Distribution) : null;
  const requestedDice = useRef<string | undefined>(undefined);
  const [diceSelection, setDiceSelection] = useState(0);
  useEffect(() => {
    if (
      view !== 'dice' ||
      !active ||
      !positionId ||
      disabled ||
      restoring ||
      pending ||
      distribution
    )
      return;
    const key = JSON.stringify([positionId, configuredEngine, plies]);
    if (requestedDice.current === key) return;
    requestedDice.current = key;
    onAnalyze('dice', plies);
  }, [
    view,
    active,
    positionId,
    disabled,
    restoring,
    pending,
    distribution,
    configuredEngine,
    plies,
    onAnalyze,
    diceSelection,
  ]);
  const chooseDice = () => {
    setView('dice');
    onPreview?.(null);
    if (!disabled && !restoring && !pending) {
      requestedDice.current = undefined;
      setDiceSelection((selection) => selection + 1);
    }
  };
  const analyze = () => {
    setView('analysis');
    selectSource('engine');
    onAnalyze('analysis', plies);
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
        <span className="engine-label source-heading-label">
          {view === 'observed'
            ? t('analysis.observed')
            : view === 'dice'
              ? (distribution?.engine ?? `${engineName} · ${t('common.local')}`)
              : selectedSource === 'file' && sources.records[0]
                ? sources.records[0].engine
                : selectedSource === 'book'
                  ? l('book')
                  : (analysis?.engine ??
                    `${engineName} · ${t('common.local')}`)}
        </span>
      </div>
      <div className="analysis-controls">
        <label className="depth-label">
          {t('analysis.depth')}{' '}
          <select
            aria-label={t('analysis.depth')}
            value={plies}
            onChange={(e) => setPlies(Number(e.target.value))}
          >
            <option value={0} disabled={!supportedPlies.includes(0)}>
              {t('analysis.fast')}
            </option>
            <option value={1} disabled={!supportedPlies.includes(1)}>
              1-ply
            </option>
            <option value={2} disabled={!supportedPlies.includes(2)}>
              {t('analysis.accurate')}
            </option>
            <option value={3} disabled={!supportedPlies.includes(3)}>
              3-ply
            </option>
            <option value={4} disabled={!supportedPlies.includes(4)}>
              4-ply
            </option>
          </select>
        </label>
        <ActionButton
          icon="analysis"
          label={t('analysis.analyze')}
          className="primary"
          disabled={disabled || pending}
          onClick={analyze}
        />
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
      <div
        className="tabs text-navigation text-navigation--compact"
        role="tablist"
        aria-label={t('analysis.views')}
      >
        <button
          role="tab"
          aria-selected={view === 'analysis'}
          onClick={() => setView('analysis')}
        >
          {t('analysis.candidates')}
        </button>
        <button role="tab" aria-selected={view === 'dice'} onClick={chooseDice}>
          {t('analysis.diceEquity')}
        </button>
        <button
          role="tab"
          aria-selected={view === 'observed'}
          onClick={() => setView('observed')}
        >
          {t(gameNo ? 'analysis.observedGame' : 'analysis.observed')}
        </button>
      </div>
      {view === 'analysis' && (
        <>
          <div className="analysis-source-tools">
            <span className="eyebrow">{l('sources')}</span>
          </div>
          <div
            className="analysis-source-tabs text-navigation text-navigation--compact"
            role="tablist"
            aria-label={l('sources')}
          >
            {(['file', 'book', 'engine'] as const).map((source) => (
              <button
                type="button"
                role="tab"
                key={source}
                aria-selected={selectedSource === source}
                tabIndex={selectedSource === source ? 0 : -1}
                ref={(element) => {
                  sourceRefs.current[source] = element;
                }}
                onKeyDown={(event) => sourceKey(event, source)}
                disabled={source === 'file' && !sources.records.length}
                onClick={() => selectSource(source)}
              >
                {l(source === 'file' ? 'imported' : source)}
              </button>
            ))}
          </div>
          {sources.loading && (
            <p className="fineprint" role="status">
              {l('loading')}
            </p>
          )}
          {sources.errors.map((error, index) => (
            <p className="analysis-source-error" key={index}>
              {formatError(error)}
            </p>
          ))}
          {selectedSource !== 'engine' && (
            <SourceAnalysisView
              records={sources.records}
              book={sources.book}
              source={selectedSource}
              onPreview={onPreview}
              selectedPreview={selectedPreview}
              position={position}
              playerName={playerName}
            />
          )}
        </>
      )}
      {view === 'analysis' &&
        selectedSource === 'engine' &&
        (analysis ? (
          <>
            <CandidateTable
              candidates={analysis.candidates.map((candidate) => ({
                ...candidate,
                equity_kind: 'cubeful',
                depth: `${analysis.plies}-ply`,
              }))}
              source={`${analysis.engine} · ${analysis.plies}-ply`}
              depth={`${analysis.plies}-ply`}
              played={position?.notation}
              position={position}
              playerName={playerName}
              onPreview={onPreview}
              selectedPreview={selectedPreview}
            />
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
                  <Tooltip
                    as="button"
                    key={i}
                    text={
                      roll
                        ? t('analysis.rollTitle', {
                            first: a,
                            second: b,
                            move: formatDecision(roll.best.notation),
                          })
                        : undefined
                    }
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
                  </Tooltip>
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
              {gameNo
                ? t('analysis.observedGameDescription', {
                    count: stats.total,
                    game: gameNo,
                  })
                : t('analysis.observedMethod', { count: stats.total })}
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
