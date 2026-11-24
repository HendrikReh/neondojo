import { useEffect, useState } from 'react';
import configurationOptions from '../../../../shared/configuration-options.json' with { type: 'json' };
import { DEFAULT_ANALYSIS_PLIES } from '../../shared/engine/analysis-depth';
import type { useJobs } from '../../shared/jobs/useJobs';
import type { MatchAnalysisEngine } from './match/useMatchAnalysis';

export type PositionView = 'analysis' | 'comparison' | 'dice' | 'observed';
export type CalculationKind = 'analysis' | 'dice';
export type PositionCalculation = ReturnType<typeof usePositionCalculation>;

/**
 * The evaluation tab of a position and the recalculation that changes it. The
 * evaluation and the header, which starts recalculations, share one instance.
 */
export function usePositionCalculation({
  positionJobs,
  positionId,
  engine = 'gnubg',
  defaultPlies = DEFAULT_ANALYSIS_PLIES,
  disabled,
  active,
  open = true,
  onShowResults,
  onResetPreview,
}: {
  positionJobs: ReturnType<typeof useJobs>;
  positionId?: string;
  engine?: MatchAnalysisEngine;
  defaultPlies?: number;
  disabled: boolean;
  active: boolean;
  /** Whether a match is open; closing it forgets the tab and the choices. */
  open?: boolean;
  onShowResults?: () => void;
  onResetPreview?: () => void;
}) {
  const { jobs, restoring, submitting, run } = positionJobs;
  const [view, setView] = useState<PositionView>('analysis');
  const [dicePlies, setDicePlies] = useState(defaultPlies);
  const [checkerPlies, setCheckerPlies] = useState(defaultPlies);
  // After a deletion, the tab waits for an explicit request instead of recalculating.
  const [deletedDice, setDeletedDice] = useState<{
    positionId?: string;
    engine: MatchAnalysisEngine;
  }>();
  useEffect(() => {
    setDicePlies(defaultPlies);
    setCheckerPlies(defaultPlies);
  }, [defaultPlies, engine, open]);
  useEffect(() => {
    if (open) return;
    setView('analysis');
    setDeletedDice(undefined);
  }, [open]);
  const [request, setRequest] = useState<{
    positionId: string;
    engine: MatchAnalysisEngine;
    plies: number;
    kind: CalculationKind;
  }>();
  /** The settings of the open recalculation dialog. */
  const settings =
    active && request?.positionId === positionId && request?.engine === engine
      ? request
      : undefined;
  useEffect(() => {
    setRequest(undefined);
  }, [active, positionId, engine]);
  const pending = Object.values(jobs).some(
    (job) => job.status === 'queued' || job.status === 'running',
  );
  const blocked = disabled || submitting || restoring || pending;
  const showDice = () => {
    setView('dice');
    setDeletedDice(undefined);
    onResetPreview?.();
  };
  return {
    engine,
    supportedPlies: configurationOptions.engine_profiles[engine].analysis,
    view,
    setView,
    showDice,
    pending,
    blocked,
    canOpen: !!positionId && !blocked,
    settings,
    diceDeleted:
      !!deletedDice &&
      deletedDice.positionId === positionId &&
      deletedDice.engine === engine,
    setDeletedDice,
    openSettings: (kind: CalculationKind) => {
      if (!positionId) return;
      setRequest({
        positionId,
        engine,
        plies: kind === 'dice' ? (jobs.dice?.plies ?? dicePlies) : checkerPlies,
        kind,
      });
    },
    setPlies: (plies: number) => settings && setRequest({ ...settings, plies }),
    closeSettings: () => setRequest(undefined),
    start: () => {
      if (!settings || blocked) return;
      setRequest(undefined);
      onShowResults?.();
      if (settings.kind === 'dice') {
        showDice();
        setDicePlies(settings.plies);
      } else {
        setView('analysis');
        setCheckerPlies(settings.plies);
      }
      void run(settings.kind, settings.plies);
    },
  };
}
