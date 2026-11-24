import { useEffect, useId, useRef, useState } from 'react';
import ActionButton from '../../../shared/ui/ActionButton';
import { useI18n } from '../../../shared/i18n/i18n';
import {
  cancelJob,
  isCancelledJob,
  terminal,
  type Job,
  type Match,
} from '../../../shared/http/api';
import PositionCalculationControls from '../PositionCalculationControls';
import type { AnalysisCatalog } from '../useAnalysisCatalog';
import type { PositionCalculation } from '../usePositionCalculation';
import AnalysisRemainingTime from './AnalysisRemainingTime';
import AnalysisTimeEstimate from './AnalysisTimeEstimate';
import {
  MatchAnalysisFields,
  useMatchAnalysisOptions,
} from './MatchAnalysisFields';
import { useMatchAnalysis, type MatchAnalysisEngine } from './useMatchAnalysis';

/**
 * Starts the analysis of the selected game and shows its progress. Mount it
 * with the match as its key and keep it mounted on every inspector tab and
 * workspace: it owns the subscription to the running job.
 */
export default function MatchAnalysisHeader({
  match,
  gameNo,
  defaultEngine,
  defaultPlies,
  configured,
  analysisInstallationKey,
  analysisRevision,
  catalog,
  positionCalculation,
  onJob,
  onAccepted,
  onAnalyzeMatch,
}: {
  match: Match;
  gameNo?: number;
  defaultEngine: MatchAnalysisEngine;
  defaultPlies: number;
  configured: boolean;
  analysisInstallationKey?: string;
  analysisRevision?: string;
  catalog: AnalysisCatalog;
  /** Recalculates the displayed position from the options, while available. */
  positionCalculation?: PositionCalculation;
  onJob?: (job: Job | undefined) => void;
  onAccepted?: (job: Job) => void;
  /** Opens the whole-match analysis dialog from the options. */
  onAnalyzeMatch?: () => void;
}) {
  const { t, formatPercent, formatError } = useI18n();
  const options = useMatchAnalysisOptions({
    defaultEngine,
    defaultPlies,
    configured,
    analysisInstallationKey,
  });
  const {
    engineId,
    plies,
    catalog: engineCatalog,
    analysisConfigured,
  } = options;
  const scoped = catalog.selected?.scope === 'game';
  const job =
    catalog.selected?.job?.kind === 'match' ? catalog.selected.job : undefined;
  const active = job && !terminal(job);
  const {
    job: gameJob,
    submitting,
    run,
    error,
  } = useMatchAnalysis(
    match.id,
    engineId,
    gameNo,
    gameNo !== undefined,
    onAccepted,
    analysisRevision,
  );
  useEffect(() => onJob?.(gameJob), [gameJob, onJob]);
  const gameRunning = gameJob && !terminal(gameJob);
  const [cancelling, setCancelling] = useState<string>();
  const [cancelError, setCancelError] = useState<unknown>();
  const [showSettings, setShowSettings] = useState(false);
  const settingsId = useId();
  const settingsButton = useRef<HTMLButtonElement>(null);
  const settingsPanel = useRef<HTMLDivElement>(null);
  // The header options float above the board; dismiss them like a menu.
  useEffect(() => {
    if (!showSettings) return;
    const inside = (target: EventTarget | null) =>
      target instanceof Node &&
      (settingsPanel.current?.contains(target) ||
        settingsButton.current?.contains(target));
    const pointer = (event: PointerEvent) => {
      if (!inside(event.target)) setShowSettings(false);
    };
    const key = (event: KeyboardEvent) => {
      // A recalculation dialog opened from the options handles its own Escape.
      if (
        event.key !== 'Escape' ||
        event.defaultPrevented ||
        settingsPanel.current?.querySelector('dialog[open]')
      )
        return;
      const refocus = inside(document.activeElement);
      setShowSettings(false);
      if (refocus) settingsButton.current?.focus();
    };
    document.addEventListener('pointerdown', pointer);
    document.addEventListener('keydown', key);
    return () => {
      document.removeEventListener('pointerdown', pointer);
      document.removeEventListener('keydown', key);
    };
  }, [showSettings]);
  // A started recalculation closes its dialog and the options together. The
  // dialog returns the focus to a button that is now hidden; this effect runs
  // after that and moves it on. A cancelled dialog returns to the options.
  const returnFocus = useRef(false);
  useEffect(() => {
    if (showSettings || !returnFocus.current) return;
    returnFocus.current = false;
    settingsButton.current?.focus();
  }, [showSettings]);
  const progress = job && (
    <div className="match-analysis-progress" role="status" aria-live="polite">
      <div>
        <strong>
          {job.status === 'completed'
            ? t(scoped ? 'gameAnalysis.completed' : 'matchAnalysis.completed')
            : isCancelledJob(job)
              ? t(scoped ? 'gameAnalysis.cancelled' : 'matchAnalysis.cancelled')
              : job.status === 'failed'
                ? t(scoped ? 'gameAnalysis.failed' : 'matchAnalysis.failed')
                : job.status === 'queued'
                  ? t('matchAnalysis.queued')
                  : t('analysis.movesRunning')}
        </strong>
        <span>
          {t('matchAnalysis.progressText', {
            completed: job.completed_games,
            total: job.total_games,
            plies: job.plies,
            progress: formatPercent(job.progress / 100),
          })}
        </span>
      </div>
      <progress
        aria-label={t('matchAnalysis.progressLabel')}
        value={job.progress}
        max={100}
      />
      {active && <small>{t('matchAnalysis.progressHint')}</small>}
      {active && <AnalysisRemainingTime key={job.id} job={job} />}
      {active && (
        <ActionButton
          icon="cancel"
          label={t('analysis.cancelJob')}
          compact
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
      )}
      {!!cancelError && <p role="alert">{formatError(cancelError)}</p>}
      {job.error && !isCancelledJob(job) && (
        <p role="alert">{t('error.engine', { details: job.error })}</p>
      )}
    </div>
  );

  return (
    <div className="workspace-analysis-command">
      <div className="match-analysis-heading">
        <div className="match-analysis-controls">
          <div className="match-analysis-actions">
            <ActionButton
              type="button"
              icon="analysis"
              label={
                submitting
                  ? t('matchAnalysis.submitting')
                  : gameRunning
                    ? t('matchAnalysis.running')
                    : gameJob
                      ? t('gameAnalysis.again')
                      : t('gameAnalysis.start')
              }
              className="primary"
              onClick={() => {
                setShowSettings(false);
                settingsButton.current?.focus();
                void run(plies);
              }}
              disabled={
                !analysisConfigured ||
                !!gameRunning ||
                submitting ||
                gameNo === undefined
              }
            />
            <ActionButton
              ref={settingsButton}
              type="button"
              icon="settings"
              label={t('analysis.matchOptions')}
              aria-expanded={showSettings}
              aria-controls={settingsId}
              onClick={() => setShowSettings((open) => !open)}
            />
          </div>
          <div
            id={settingsId}
            ref={settingsPanel}
            className="match-analysis-popover"
            hidden={!showSettings}
          >
            <div className="match-analysis-options">
              <MatchAnalysisFields
                options={options}
                disabled={submitting || !configured}
              />
              <div className="position-analysis-actions">
                {positionCalculation && (
                  <PositionCalculationControls
                    calculation={positionCalculation}
                    onCalculationStarted={() => {
                      returnFocus.current = true;
                      setShowSettings(false);
                    }}
                  />
                )}
              </div>
            </div>
            {showSettings && !gameRunning && !submitting && (
              <AnalysisTimeEstimate
                key={match.position_count}
                matchId={match.id}
                gameNo={gameNo}
                scoped
                engineId={engineId}
                plies={plies}
                configured={analysisConfigured}
                installationKey={analysisInstallationKey}
              />
            )}
            {onAnalyzeMatch && (
              <div className="match-analysis-whole">
                <ActionButton
                  type="button"
                  icon="analysis"
                  label={`${t('matchAnalysis.start')} …`}
                  aria-haspopup="dialog"
                  onClick={() => {
                    // The dialog returns focus to the options button it hides.
                    setShowSettings(false);
                    settingsButton.current?.focus();
                    onAnalyzeMatch();
                  }}
                />
              </div>
            )}
          </div>
          {!!engineCatalog?.error && (
            <p className="analysis-error" role="alert">
              {formatError(engineCatalog.error)}
            </p>
          )}
        </div>
      </div>
      {progress}
      {!!error && (
        <p className="analysis-error" role="alert">
          {formatError(error)}
        </p>
      )}
    </div>
  );
}
