import { useEffect, useEffectEvent, useId, useState } from 'react';
import type { PositionCalculation } from './usePositionCalculation';
import {
  request,
  type Distribution,
  type DiceStats,
  type Match,
  type Position,
} from '../../shared/http/api';
import type { useJobs } from '../../shared/jobs/useJobs';
import { useI18n } from '../../shared/i18n/i18n';
import { TabList } from '../../shared/ui/TabList';
import { useOpeningBook } from './candidates/useOpeningBook';
import { useSourceText } from './candidates/analysis-source-messages';
import SourceAnalysisView from './candidates/SourceAnalysisView';
import type { PreviewCandidate } from '../../shared/review/candidate-preview';
import type { BoardView } from '../../shared/board/move-preview';
import PositionComparison from './candidates/PositionComparison';
import { useAnalysisResult, type AnalysisCatalog } from './useAnalysisCatalog';
import { useAnalysisCatalogText } from './analysis-catalog-messages';
import { useCandidateText } from './candidates/candidate-messages';
import './candidates/analysis-sources.css';
import { DEFAULT_ANALYSIS_PLIES } from '../../shared/engine/analysis-depth';
import type { SourceAnalysis } from './candidates/source-analysis-api';
import DiceEquityTab from './DiceEquityTab';
import { SelectedAnalysisResult } from './SelectedAnalysisResult';

export { SelectedAnalysisResult };

const noImportedRecords: SourceAnalysis[] = [];
type AnalysisPanelView = PositionCalculation['view'];

export default function AnalysisPanel({
  positionJobs,
  calculation,
  catalog,
  observedMatchId,
  gameNo,
  active = true,
  resultsVisible,
  defaultPlies = DEFAULT_ANALYSIS_PLIES,
  positionId,
  openingBookRevision = 0,
  onPreview,
  selectedPreview,
  position,
  playerName,
  players,
  analysisRevision,
}: {
  positionJobs: ReturnType<typeof useJobs>;
  calculation: PositionCalculation;
  catalog: AnalysisCatalog;
  observedMatchId?: string;
  gameNo?: number;
  active?: boolean;
  resultsVisible: boolean;
  defaultPlies?: number;
  positionId?: string;
  openingBookRevision?: number;
  onPreview?: (candidate: PreviewCandidate | null, view?: BoardView) => void;
  selectedPreview?: PreviewCandidate;
  position?: Position;
  playerName?: string;
  players?: Pick<Match, 'white' | 'black'>;
  analysisRevision?: string;
}) {
  const { t, formatNumber, formatError } = useI18n();
  const panelId = useId();
  const { jobs, restoring, failure, submitting, run, removeDice } =
    positionJobs;
  const { view, setView, pending, blocked, diceDeleted } = calculation;
  const l = useSourceText();
  const candidateText = useCandidateText();
  const catalogText = useAnalysisCatalogText();
  const book = useOpeningBook(positionId, openingBookRevision);
  const saved = useAnalysisResult(
    positionId,
    catalog.analysisId,
    analysisRevision,
    catalog.sourceRevision,
  );
  const resetPreview = useEffectEvent(() => onPreview?.(null));
  const [bookOpen, setBookOpen] = useState(false);
  const observedPath =
    observedMatchId && gameNo !== undefined
      ? `/matches/${encodeURIComponent(observedMatchId)}/dice?game_no=${gameNo}`
      : undefined;
  const [observed, setObserved] = useState<
    { key: string; stats: DiceStats } | { key: string; error: unknown }
  >();
  const currentObserved = observed?.key === observedPath ? observed : undefined;
  const stats =
    currentObserved && 'stats' in currentObserved
      ? currentObserved.stats
      : undefined;
  const loadObserved =
    resultsVisible && view === 'observed' && !currentObserved;
  useEffect(() => {
    if (!observedPath) {
      setObserved(undefined);
      return;
    }
    if (!loadObserved) return;
    const abort = new AbortController();
    void request<DiceStats>(observedPath, { signal: abort.signal }).then(
      (stats) => {
        if (!abort.signal.aborted) setObserved({ key: observedPath, stats });
      },
      (error: unknown) => {
        if (!abort.signal.aborted) setObserved({ key: observedPath, error });
      },
    );
    return () => abort.abort();
  }, [observedPath, loadObserved]);
  useEffect(() => resetPreview(), [catalog.analysisId, positionId, view]);

  const diceJob = jobs.dice;
  const latestJob = view === 'dice' ? diceJob : jobs.analysis;
  const distribution =
    diceJob?.status === 'completed' ? (diceJob.result as Distribution) : null;
  const canRequestDice =
    resultsVisible &&
    !!positionId &&
    !blocked &&
    failure !== 'restore' &&
    !calculation.settings;
  const diceFailed = failure === 'dice' || diceJob?.status === 'failed';
  const requestMissingDice = useEffectEvent(() => {
    void run('dice', defaultPlies);
  });
  useEffect(() => {
    if (
      view === 'dice' &&
      canRequestDice &&
      !diceJob &&
      !diceFailed &&
      !diceDeleted
    )
      requestMissingDice();
  }, [view, canRequestDice, diceJob, diceFailed, diceDeleted]);

  const selectView = (next: AnalysisPanelView) => {
    if (next === 'comparison') {
      onPreview?.(null);
      setView('comparison');
    } else if (next === 'dice') {
      calculation.showDice();
      if (canRequestDice && !distribution && diceFailed)
        void run('dice', defaultPlies);
    } else {
      setView(next);
    }
  };

  return (
    <section className="analysis-panel">
      <div className="position-analysis-toolbar">
        <TabList<AnalysisPanelView>
          className="tabs text-navigation text-navigation--compact"
          ariaLabel={t('analysis.views')}
          active={view}
          onSelect={selectView}
          tabs={[
            {
              value: 'analysis',
              id: `${panelId}-tab-analysis`,
              controls: `${panelId}-panel-analysis`,
              label: t('analysis.candidates'),
            },
            {
              value: 'comparison',
              id: `${panelId}-tab-comparison`,
              controls: `${panelId}-panel-comparison`,
              label: candidateText('compareMoves'),
              disabled: !position,
            },
            {
              value: 'dice',
              id: `${panelId}-tab-dice`,
              controls: `${panelId}-panel-dice`,
              label: t('analysis.diceEquity'),
            },
            {
              value: 'observed',
              id: `${panelId}-tab-observed`,
              controls: `${panelId}-panel-observed`,
              label: t(gameNo ? 'analysis.observedGame' : 'analysis.observed'),
            },
          ]}
        />
      </div>
      <div
        id={`${panelId}-panel-analysis`}
        role="tabpanel"
        aria-labelledby={`${panelId}-tab-analysis`}
        hidden={view !== 'analysis'}
      >
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
      </div>
      <div
        id={`${panelId}-panel-comparison`}
        role="tabpanel"
        aria-labelledby={`${panelId}-tab-comparison`}
        hidden={view !== 'comparison'}
      >
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
      </div>
      <div
        id={`${panelId}-panel-dice`}
        role="tabpanel"
        aria-labelledby={`${panelId}-tab-dice`}
        hidden={view !== 'dice'}
      >
        {view === 'dice' && (
          <DiceEquityTab
            positionId={positionId}
            position={position}
            diceJob={diceJob}
            distribution={distribution}
            restoring={restoring}
            submitting={submitting}
            pending={pending}
            blocked={blocked}
            diceDeleted={diceDeleted}
            hasJobError={Boolean(positionJobs.error)}
            calculation={calculation}
            removeDice={removeDice}
            onPreview={onPreview}
          />
        )}
      </div>
      <div
        id={`${panelId}-panel-observed`}
        role="tabpanel"
        aria-labelledby={`${panelId}-tab-observed`}
        hidden={view !== 'observed'}
      >
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
          ) : currentObserved && 'error' in currentObserved ? (
            <div className="panel-empty">
              <p className="error" role="alert">
                {formatError(currentObserved.error)}
              </p>
              <button type="button" onClick={() => setObserved(undefined)}>
                {t('versions.retry')}
              </button>
            </div>
          ) : (
            <div className="panel-empty">
              <p role={observedPath ? 'status' : undefined}>
                {t(observedPath ? 'versions.loading' : 'analysis.selectMatch')}
              </p>
            </div>
          ))}
      </div>
      {view !== 'observed' && latestJob?.status === 'failed' && (
        <p className="error" role="alert">
          <strong>{t('analysis.failed')}</strong>{' '}
          {latestJob.error && t('error.engine', { details: latestJob.error })}
        </p>
      )}
    </section>
  );
}
