import { useOptionalConfiguration } from './ConfigurationProvider';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import Tooltip from './Tooltip';
import ActionButton from './ActionButton';
import LibraryDialog from './LibraryDialog';
import { useLibraryText } from './library-messages';
import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import {
  type Analysis,
  type Distribution,
  type DiceStats,
  type Match,
  type Position,
  type Job,
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
  completedAnalysis,
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
  players,
  preferredSource,
}: {
  jobs: Jobs;
  completedAnalysis?: Job;
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
  players?: Pick<Match, 'white' | 'black'>;
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
  const library = useLibraryText();
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
      : completedAnalysis
        ? 'engine'
        : sources.records.length
          ? 'file'
          : sources.book?.hit
            ? 'book'
            : 'engine';
  useEffect(() => onPreview?.(null), [selectedSource, positionId, onPreview]);
  type Source = 'file' | 'book' | 'engine';
  function selectSource(source: Source) {
    setRequestedAnalysis(undefined);
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
  const [dicePlies, setDicePlies] = useState(defaultPlies);
  const [checkerPlies, setCheckerPlies] = useState(defaultPlies);
  useEffect(() => {
    setDicePlies(defaultPlies);
    setCheckerPlies(defaultPlies);
  }, [defaultPlies, configuredEngine]);
  const [view, setView] = useState<'analysis' | 'dice' | 'observed'>(
    'analysis',
  );
  const calculationRef = useRef<HTMLButtonElement>(null);
  const [calculation, setCalculation] = useState<{
    positionId: string;
    engine: string;
    plies: number;
    kind: 'analysis' | 'dice';
  }>();
  const settings =
    active &&
    calculation?.positionId === positionId &&
    calculation?.engine === configuredEngine
      ? calculation
      : undefined;
  useEffect(() => {
    setCalculation(undefined);
  }, [active, positionId, configuredEngine]);
  const [requestedAnalysis, setRequestedAnalysis] = useState<{
    positionId: string;
    engine: string;
    previousResult?: string;
    previousJob?: string;
  }>();
  const [selectedRoll, setSelectedRoll] = useState('1-1');
  const diceJob = jobs.dice?.plies === dicePlies ? jobs.dice : undefined;
  const latestJob = view === 'dice' ? diceJob : jobs.analysis;
  const pending = Object.values(jobs).some(
    (job) => job.status === 'queued' || job.status === 'running',
  );
  const analysis = (completedAnalysis?.result as Analysis | undefined) ?? null;
  useEffect(() => {
    if (!requestedAnalysis) return;
    if (
      requestedAnalysis.positionId !== positionId ||
      requestedAnalysis.engine !== configuredEngine
    ) {
      setRequestedAnalysis(undefined);
      return;
    }
    if (
      jobs.analysis?.status === 'failed' &&
      jobs.analysis.id !== requestedAnalysis.previousJob
    ) {
      setRequestedAnalysis(undefined);
      return;
    }
    if (
      completedAnalysis &&
      completedAnalysis.id !== requestedAnalysis.previousResult
    ) {
      setRequestedAnalysis(undefined);
      setSourceChoice({ positionId, source: 'engine' });
    }
  }, [
    completedAnalysis,
    configuredEngine,
    jobs.analysis,
    positionId,
    requestedAnalysis,
  ]);
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
    const key = JSON.stringify([positionId, configuredEngine, dicePlies]);
    if (requestedDice.current === key) return;
    requestedDice.current = key;
    onAnalyze('dice', dicePlies);
  }, [
    view,
    active,
    positionId,
    disabled,
    restoring,
    pending,
    distribution,
    configuredEngine,
    dicePlies,
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
  const closeCalculation = () => {
    setCalculation(undefined);
    requestAnimationFrame(() => calculationRef.current?.focus());
  };
  const startCalculation = () => {
    if (!settings || disabled || restoring || pending) return;
    closeCalculation();
    if (settings.kind === 'dice') {
      setDicePlies(settings.plies);
      requestedDice.current = JSON.stringify([
        positionId,
        configuredEngine,
        settings.plies,
      ]);
    } else {
      setCheckerPlies(settings.plies);
      setRequestedAnalysis({
        positionId: settings.positionId,
        engine: configuredEngine,
        previousResult: completedAnalysis?.id,
        previousJob: jobs.analysis?.id,
      });
    }
    onAnalyze(settings.kind, settings.plies);
  };
  const calculationTitle = t(
    view === 'dice' ? 'analysis.recalculateDice' : 'analysis.recalculate',
  );
  const best = distribution?.rolls.find(
    (r) => r.dice.join('-') === selectedRoll,
  );
  return (
    <section className="analysis-panel">
      <div className="position-analysis-toolbar">
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
          <button
            role="tab"
            aria-selected={view === 'dice'}
            onClick={chooseDice}
          >
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
        {view !== 'observed' && (
          <div className="position-calculation-action">
            <ActionButton
              ref={calculationRef}
              type="button"
              icon="analysis"
              label={`${calculationTitle} …`}
              aria-haspopup="dialog"
              aria-disabled={disabled || pending || restoring || undefined}
              onClick={() => {
                if (positionId && !disabled && !pending && !restoring)
                  setCalculation({
                    positionId,
                    plies: view === 'dice' ? dicePlies : checkerPlies,
                    engine: configuredEngine,
                    kind: view === 'dice' ? 'dice' : 'analysis',
                  });
              }}
            />
          </div>
        )}
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
              players={players}
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
        ) : sources.loading || restoring ? null : (
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
              <span className="engine-label">{distribution.engine}</span>
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
      {view !== 'observed' && latestJob?.status === 'failed' && (
        <p className="error" role="alert">
          <strong>{t('analysis.failed')}</strong>{' '}
          {latestJob.error && t('error.engine', { details: latestJob.error })}
        </p>
      )}
      {settings && (
        <LibraryDialog
          title={t(
            settings.kind === 'dice'
              ? 'analysis.recalculateDice'
              : 'analysis.recalculate',
          )}
          native
          initialFocus="dialog"
          onClose={closeCalculation}
        >
          <p className="fineprint">{engineName}</p>
          <div className="analysis-controls">
            <PositionAnalysisDepth
              plies={settings.plies}
              supportedPlies={supportedPlies}
              onChange={(plies) => setCalculation({ ...settings, plies })}
            />
            <ActionButton
              type="button"
              icon="analysis"
              label={t(
                settings.kind === 'dice'
                  ? 'analysis.diceEquity'
                  : 'analysis.analyze',
              )}
              className="primary"
              disabled={disabled || pending || restoring}
              onClick={startCalculation}
            />
            <button type="button" onClick={closeCalculation}>
              {library('cancel')}
            </button>
          </div>
        </LibraryDialog>
      )}
    </section>
  );
}

export function PositionAnalysisDepth({
  plies = DEFAULT_ANALYSIS_PLIES,
  supportedPlies,
  onChange,
}: {
  plies?: number;
  supportedPlies: readonly number[];
  onChange: (plies: number) => void;
}) {
  const { t } = useI18n();
  return (
    <label className="depth-label">
      {t('analysis.depth')}{' '}
      <select
        aria-label={t('analysis.depth')}
        value={plies}
        onChange={(event) => onChange(Number(event.target.value))}
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
  );
}
