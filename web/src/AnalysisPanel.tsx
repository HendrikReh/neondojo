import type { MatchAnalysisEngine } from './useMatchAnalysis';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import Tooltip from './Tooltip';
import EngineLabel from './EngineLabel';
import ActionButton from './ActionButton';
import LibraryDialog from './LibraryDialog';
import { useLibraryText } from './library-messages';
import { useEffect, useEffectEvent, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  type Distribution,
  type DiceStats,
  type Match,
  type Position,
} from './api';
import type { Jobs } from './useJobs';
import { useI18n } from './i18n';
import { useOpeningBook } from './useOpeningBook';
import { useSourceText } from './analysis-source-messages';
import SourceAnalysisView from './SourceAnalysisView';
import type { PreviewCandidate } from './candidate-preview';
import type { BoardView } from './move-preview';
import CandidateTable from './CandidateTable';
import PositionComparison from './PositionComparison';
import { nativeCheckerCandidates } from './candidate-comparison';
import { BoardDecisionComparison } from './BoardDecisionReview';
import { useAnalysisResult, type AnalysisCatalog } from './useAnalysisCatalog';
import type { PositionAnalysisResult } from './analysis-catalog-api';
import { useAnalysisCatalogText } from './analysis-catalog-messages';
import { useCandidateText } from './candidate-messages';
import './analysis-sources.css';
import { DEFAULT_ANALYSIS_PLIES } from './analysis-depth';
import type { SourceAnalysis } from './source-analysis-api';

const noImportedRecords: SourceAnalysis[] = [];

export default function AnalysisPanel({
  jobs,
  catalog,
  stats,
  gameNo,
  disabled,
  active = true,
  calculationTarget,
  onShowResults,
  restoring = false,
  onAnalyze,
  defaultPlies = DEFAULT_ANALYSIS_PLIES,
  configuredEngine = 'gnubg',
  positionId,
  sourceRevision = 0,
  onPreview,
  selectedPreview,
  position,
  playerName,
  players,
  analysisRevision,
}: {
  jobs: Jobs;
  catalog: AnalysisCatalog;
  stats: DiceStats | null;
  gameNo?: number;
  disabled: boolean;
  active?: boolean;
  calculationTarget?: HTMLElement | null;
  onShowResults?: () => void;
  restoring?: boolean;
  onAnalyze: (kind: 'analysis' | 'dice', plies: number) => void;
  defaultPlies?: number;
  configuredEngine?: MatchAnalysisEngine;
  positionId?: string;
  sourceRevision?: number;
  onPreview?: (candidate: PreviewCandidate | null, view?: BoardView) => void;
  selectedPreview?: PreviewCandidate;
  position?: Position;
  playerName?: string;
  players?: Pick<Match, 'white' | 'black'>;
  analysisRevision?: string;
}) {
  const {
    t,
    formatNumber,
    formatPercent,
    formatEquity,
    formatDecision,
    formatError,
  } = useI18n();
  const engineName = configuredEngine === 'bgsage' ? 'Open Sage' : 'GNUbg';
  const supportedPlies =
    configurationOptions.engine_profiles[configuredEngine].analysis;
  const l = useSourceText();
  const candidateText = useCandidateText();
  const catalogText = useAnalysisCatalogText();
  const library = useLibraryText();
  const book = useOpeningBook(positionId, sourceRevision);
  const saved = useAnalysisResult(
    positionId,
    catalog.analysisId,
    analysisRevision,
  );
  const resetPreview = useEffectEvent(() => onPreview?.(null));
  const [bookOpen, setBookOpen] = useState(false);
  const [dicePlies, setDicePlies] = useState(defaultPlies);
  const [checkerPlies, setCheckerPlies] = useState(defaultPlies);
  useEffect(() => {
    setDicePlies(defaultPlies);
    setCheckerPlies(defaultPlies);
  }, [defaultPlies, configuredEngine]);
  const [view, setView] = useState<
    'analysis' | 'comparison' | 'dice' | 'observed'
  >('analysis');
  useEffect(() => resetPreview(), [catalog.analysisId, positionId, view]);
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
  const [selectedRoll, setSelectedRoll] = useState('1-1');
  const diceJob = jobs.dice;
  const latestJob = view === 'dice' ? diceJob : jobs.analysis;
  const pending = Object.values(jobs).some(
    (job) => job.status === 'queued' || job.status === 'running',
  );
  const distribution =
    diceJob?.status === 'completed' ? (diceJob.result as Distribution) : null;
  const chooseDice = () => {
    setView('dice');
    onPreview?.(null);
  };
  const closeCalculation = () => setCalculation(undefined);
  const startCalculation = () => {
    if (!settings || disabled || restoring || pending) return;
    closeCalculation();
    onShowResults?.();
    if (settings.kind === 'dice') {
      chooseDice();
      setDicePlies(settings.plies);
    } else {
      setView('analysis');
      setCheckerPlies(settings.plies);
    }
    onAnalyze(settings.kind, settings.plies);
  };
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
            aria-selected={view === 'comparison'}
            disabled={!position}
            onClick={() => {
              onPreview?.(null);
              setView('comparison');
            }}
          >
            {candidateText('compareMoves')}
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
      </div>
      {view === 'analysis' && (
        <>
          {jobs.analysis?.status === 'completed' &&
            catalog.analysisId !== `job:${jobs.analysis.id}` && (
              <button
                type="button"
                disabled={catalog.saving}
                onClick={() => void catalog.select(`job:${jobs.analysis!.id}`)}
              >
                {catalogText('showCalculated')}
              </button>
            )}
          {saved.loading || catalog.loading || catalog.saving ? (
            <p className="fineprint" role="status">
              {l('loading')}
            </p>
          ) : saved.error ? (
            <p className="analysis-source-error" role="alert">
              {formatError(saved.error)}
            </p>
          ) : (
            <SelectedAnalysisResult
              result={saved.result}
              selected={!!catalog.selected || !!catalog.selectedId}
              alternatives={catalog.entries.some(
                (entry) => entry.id !== catalog.analysisId,
              )}
              onPreview={onPreview}
              selectedPreview={selectedPreview}
              position={position}
              playerName={playerName}
              players={players}
            />
          )}
          <details
            className="analysis-book-reference"
            onToggle={(event) => {
              setBookOpen(event.currentTarget.open);
              onPreview?.(null);
            }}
          >
            <summary>{l('book')}</summary>
            {book.loading && <p role="status">{l('loading')}</p>}
            {!!book.error && <p role="alert">{formatError(book.error)}</p>}
            {bookOpen && (
              <SourceAnalysisView
                records={noImportedRecords}
                book={book.book}
                source="book"
                onPreview={onPreview}
                selectedPreview={selectedPreview}
                position={position}
                playerName={playerName}
                players={players}
              />
            )}
          </details>
        </>
      )}
      {view === 'comparison' && position && (
        <PositionComparison
          key={position.match_uuid}
          position={position}
          catalog={catalog}
          active={active}
          revision={analysisRevision}
          onPreview={
            onPreview && ((candidate) => onPreview(candidate, 'arrows'))
          }
          selectedPreview={selectedPreview}
        />
      )}
      {view === 'dice' &&
        (distribution ? (
          <div className="distribution">
            <div className="distribution-caption">
              <span>
                <EngineLabel
                  details={`${distribution.engine} · ${distribution.plies}-ply`}
                />
              </span>
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
      {active &&
        calculationTarget &&
        createPortal(
          <>
            {(['analysis', 'dice'] as const).map((kind) => (
              <ActionButton
                key={kind}
                type="button"
                icon="analysis"
                label={`${t(kind === 'dice' ? 'analysis.recalculateDice' : 'analysis.recalculate')} …`}
                aria-haspopup="dialog"
                disabled={!positionId || disabled || pending || restoring}
                onClick={(event) => {
                  if (!positionId) return;
                  event.currentTarget.focus();
                  setCalculation({
                    positionId,
                    plies:
                      kind === 'dice'
                        ? (diceJob?.plies ?? dicePlies)
                        : checkerPlies,
                    engine: configuredEngine,
                    kind,
                  });
                }}
              />
            ))}
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
          </>,
          calculationTarget,
        )}
    </section>
  );
}

export function SelectedAnalysisResult({
  result,
  selected,
  alternatives,
  onPreview,
  selectedPreview,
  position,
  playerName,
  players,
}: {
  result?: PositionAnalysisResult | null;
  selected: boolean;
  alternatives: boolean;
  onPreview?: (candidate: PreviewCandidate | null) => void;
  selectedPreview?: PreviewCandidate;
  position?: Position;
  playerName?: string;
  players?: Pick<Match, 'white' | 'black'>;
}) {
  const text = useAnalysisCatalogText();
  const candidateText = useCandidateText();
  const analysis = result?.analysis;
  const decision = result?.decision;
  const candidates = nativeCheckerCandidates(result);
  const depth = analysis ? `${analysis.plies}-ply` : result?.depth;
  const source = [analysis?.engine ?? result?.engine, depth]
    .filter(Boolean)
    .join(' · ');
  if (candidates.length)
    return (
      <CandidateTable
        candidates={candidates.map((candidate) => ({
          ...candidate,
          depth: candidate.depth ?? depth,
        }))}
        source={source}
        depth={depth}
        played={decision?.played ?? position?.notation}
        position={position}
        playerName={playerName}
        onPreview={onPreview}
        selectedPreview={selectedPreview}
      />
    );
  if (result?.records.length)
    return (
      <SourceAnalysisView
        records={result.records}
        source="file"
        onPreview={onPreview}
        selectedPreview={selectedPreview}
        position={position}
        playerName={playerName}
        players={players}
      />
    );
  if (decision)
    return (
      <BoardDecisionComparison
        played={decision.played}
        best={decision.best}
        equities={{
          played: decision.played_equity,
          best: decision.best_equity,
        }}
        loss={
          decision.evaluation_status === 'evaluated'
            ? decision.equity_loss
            : null
        }
        context={candidateText('normalizedCubeful')}
        source={source}
      />
    );
  return (
    <div className="panel-empty">
      <p>{text(selected ? 'missing' : 'none')}</p>
      {alternatives && <p>{text('alternatives')}</p>}
    </div>
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
