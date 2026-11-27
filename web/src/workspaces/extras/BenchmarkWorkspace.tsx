import { useMemo, useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import { formatPly } from '../../shared/format/ply';
import { useI18n } from '../../shared/i18n/i18n';
import BenchmarkComparison from './BenchmarkComparison';
import BenchmarkHistoryTable from './BenchmarkHistoryTable';
import BenchmarkSetupForm from './BenchmarkSetupForm';
import { useBenchmarkText } from './benchmark-messages';
import {
  benchmarkProgress,
  benchmarkStatusLabel,
  useBenchmarkRuns,
} from './useBenchmarkRuns';
import './benchmark.css';

export default function BenchmarkWorkspace({
  visible,
  defaultPlies,
  playPlies,
  activeElsewhere = false,
  onActivity,
}: {
  visible: boolean;
  defaultPlies: number;
  playPlies: number | undefined;
  /** The global status knows of a queued or running run. */
  activeElsewhere?: boolean;
  /** Reports whether the queue is active after each read and start. */
  onActivity?: (active: boolean) => void;
}) {
  const t = useBenchmarkText();
  const { formatNumber, formatError } = useI18n();
  const [mode, setMode] = useState<'analysis' | 'play'>('analysis');
  const {
    reference,
    engines,
    engineId,
    setEngineId,
    engine,
    runs,
    activeRuns,
    loading,
    queueActive,
    canStart,
    referenceError,
    engineError,
    historyError,
    actionError,
    starting,
    cancelling,
    deleting,
    startAnalysis,
    startPlay,
    cancel,
    remove,
    retry,
  } = useBenchmarkRuns({ visible, activeElsewhere, onActivity });

  const play = mode === 'play';
  const modeRuns = useMemo(
    () => runs.filter((run) => Boolean(run.play_reference) === play),
    [runs, play],
  );
  const error =
    actionError ??
    engineError ??
    historyError ??
    (play ? undefined : referenceError);

  return (
    <div className="benchmark-workspace">
      <header className="benchmark-heading">
        <h1 className="sr-only">{t('title')}</h1>
        <p>{t('description')}</p>
      </header>
      {error !== undefined && (
        <div className="error" role="alert">
          {formatError(error)}{' '}
          <ActionButton
            icon="refresh"
            label={t('retry')}
            compact
            type="button"
            onClick={retry}
          />
        </div>
      )}
      <BenchmarkSetupForm
        mode={mode}
        onModeChange={setMode}
        engines={engines}
        engine={engine}
        engineId={engineId}
        onEngineChange={setEngineId}
        engineError={engineError}
        reference={reference}
        referenceError={referenceError}
        defaultPlies={defaultPlies}
        playPlies={playPlies}
        canStart={canStart}
        starting={starting}
        onStartAnalysis={(plies) => void startAnalysis(plies)}
        onStartPlay={(depth) => void startPlay(depth)}
      />
      {queueActive && (
        <p className="benchmark-help" role="status">
          {t('engineBusy')}
        </p>
      )}
      {activeRuns.length > 0 && (
        <section className="benchmark-active" aria-label={t('active')}>
          <h2>{t('active')}</h2>
          {activeRuns.map((run) => {
            const progress = benchmarkProgress(run, t, formatNumber);
            return (
              <article key={run.id}>
                <div>
                  <strong>
                    {t(run.play_reference ? 'playLatency' : 'matchAnalysis')} ·{' '}
                    {run.engine_name} · {formatPly(run.plies, formatNumber)} ·{' '}
                    {benchmarkStatusLabel(run.status, t)}
                  </strong>
                  <p>{progress}</p>
                  {(run.status === 'queued' || !run.started_at) && (
                    <p>{t('waiting')}</p>
                  )}
                  <progress
                    max={
                      run.play_reference?.total_turns ??
                      run.reference?.total_games ??
                      1
                    }
                    value={
                      run.play_reference
                        ? run.completed_turns
                        : run.completed_games
                    }
                    aria-label={progress}
                  />
                </div>
                <ActionButton
                  icon="cancel"
                  type="button"
                  label={t(cancelling === run.id ? 'cancelling' : 'cancel')}
                  disabled={Boolean(cancelling)}
                  onClick={() => void cancel(run.id)}
                />
              </article>
            );
          })}
        </section>
      )}
      {!play && (
        <section className="benchmark-reference" aria-label={t('reference')}>
          {reference ? (
            <>
              <p>
                <strong>{reference.name}</strong>
                {' · '}
                {t('referenceFacts', {
                  points: reference.match_length,
                  games: reference.total_games,
                  moves: reference.total_moves,
                })}
              </p>
              <details>
                <summary>{t('methodology')}</summary>
                <p>
                  {t('referenceVersion', {
                    version: formatNumber(reference.version),
                  })}{' '}
                  · {reference.id}
                </p>
                {engineId === 'gnubg' && (
                  <p>
                    {t('protocol')}: {reference.protocol.id} ·{' '}
                    {reference.protocol.game_timeout_seconds == null
                      ? t('noTimeout')
                      : t('timeout', {
                          seconds: reference.protocol.game_timeout_seconds,
                        })}
                  </p>
                )}
                <p>
                  SHA-256 <code>{reference.sha256}</code>
                </p>
                <p>{t('timing')}</p>
              </details>
            </>
          ) : referenceError === undefined ? (
            <p role="status">{t('loading')}</p>
          ) : null}
        </section>
      )}
      <BenchmarkComparison runs={modeRuns} />
      <BenchmarkHistoryTable
        runs={modeRuns}
        play={play}
        loading={loading}
        deleting={deleting}
        onDelete={remove}
      />
    </div>
  );
}
