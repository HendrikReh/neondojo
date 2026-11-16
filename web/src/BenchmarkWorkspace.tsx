import BenchmarkComparison, {
  BenchmarkInitialization,
} from './BenchmarkComparison';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import { useEffect, useRef, useState } from 'react';
import EngineLabel from './EngineLabel';
import { ANALYSIS_PLIES } from './analysis-depth';
import { formatBenchmarkDuration } from './benchmark-duration';
import { useI18n } from './i18n';
import { loadEngines, type EngineInfo } from './settings-api';
import {
  cancelBenchmark,
  loadBenchmarkReference,
  loadBenchmarks,
  startBenchmarks,
  startPlayBenchmark,
  type BenchmarkReference,
  type BenchmarkRun,
} from './benchmark-api';
import { useBenchmarkText } from './benchmark-messages';
import './benchmark.css';

function preferredDepth(depths: readonly number[]) {
  return depths.includes(2) ? 2 : depths[0];
}

export default function BenchmarkWorkspace({
  visible,
  defaultPlies,
  playPlies,
}: {
  visible: boolean;
  defaultPlies: number;
  playPlies: number | undefined;
}) {
  const t = useBenchmarkText();
  const { locale, formatNumber, formatError } = useI18n();
  const [mode, setMode] = useState<'analysis' | 'play'>('analysis');
  const [reference, setReference] = useState<BenchmarkReference>();
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [engineId, setEngineId] = useState('gnubg');
  const [playDepthsByEngine, setPlayDepthsByEngine] = useState<
    Record<string, number>
  >({});
  const [analysisDepthsByEngine, setAnalysisDepthsByEngine] = useState<
    Record<string, number[]>
  >({});
  const [runs, setRuns] = useState<BenchmarkRun[]>([]);
  const [loading, setLoading] = useState(true);
  const [referenceError, setReferenceError] = useState<unknown>();
  const [engineError, setEngineError] = useState<unknown>();
  const [historyError, setHistoryError] = useState<unknown>();
  const [actionError, setActionError] = useState<unknown>();
  const [starting, setStarting] = useState<'analysis' | 'play'>();
  const [cancelling, setCancelling] = useState<string>();
  const [revision, setRevision] = useState(0);
  const [configurationRevision, setConfigurationRevision] = useState(0);
  const [queueActive, setQueueActive] = useState(false);
  const previousActive = useRef(false);
  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    setReferenceError(undefined);
    setEngineError(undefined);
    void loadBenchmarkReference(abort.signal)
      .then((nextReference) => {
        if (!abort.signal.aborted) setReference(nextReference);
      })
      .catch((error) => {
        if (!abort.signal.aborted) setReferenceError(error);
      });
    void loadEngines(abort.signal)
      .then((catalog) => {
        if (abort.signal.aborted) return;
        setEngines(catalog);
        setEngineId((current) =>
          catalog.some((e) => e.id === current && e.available)
            ? current
            : (catalog.find((e) => e.available)?.id ?? ''),
        );
      })
      .catch((error) => {
        if (!abort.signal.aborted) setEngineError(error);
      });
    return () => abort.abort();
  }, [visible, configurationRevision]);
  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const result = await loadBenchmarks(abort.signal);
        if (abort.signal.aborted) return;
        setRuns(result.runs);
        setQueueActive(result.active);
        if (previousActive.current && !result.active)
          setConfigurationRevision((r) => r + 1);
        previousActive.current = result.active;
        setHistoryError(undefined);
      } catch (error) {
        if (!abort.signal.aborted) setHistoryError(error);
      } finally {
        if (!abort.signal.aborted) {
          setLoading(false);
          timer = setTimeout(() => void poll(), 1000);
        }
      }
    }
    void poll();
    return () => {
      abort.abort();
      clearTimeout(timer);
    };
  }, [visible, revision]);
  const engine = engines.find((e) => e.id === engineId && e.available);
  const requestedAnalysisDepths = analysisDepthsByEngine[engineId] ?? [
    defaultPlies,
  ];
  const availableAnalysisDepths = engine?.plies ?? [];
  const compatiblePlies = requestedAnalysisDepths
    .filter((ply) => availableAnalysisDepths.includes(ply))
    .sort((a, b) => a - b);
  const analysisFallback =
    requestedAnalysisDepths.length > 0 && compatiblePlies.length === 0
      ? preferredDepth(availableAnalysisDepths)
      : undefined;
  const selected = analysisFallback === undefined
    ? compatiblePlies
    : [analysisFallback];
  const play = mode === 'play';
  const modeRuns = runs.filter((run) => Boolean(run.play_reference) === play);
  const supportedPlayDepths = (
    engineId === 'bgsage'
      ? configurationOptions.engine_profiles.bgsage.play
      : configurationOptions.engine_profiles.gnubg.play
  ).filter((ply) => engine?.plies.includes(ply));
  const requestedPlayDepth = playDepthsByEngine[engineId] ?? playPlies ?? 2;
  const playDepthSupported = supportedPlayDepths.includes(requestedPlayDepth);
  const playFallback = playDepthSupported
    ? undefined
    : preferredDepth(supportedPlayDepths);
  const selectedPlayDepth = playDepthSupported
    ? requestedPlayDepth
    : playFallback;
  const active = runs.filter(
    (run) => run.status === 'running' || run.status === 'queued',
  );
  const canStart =
    !starting && !queueActive && !loading && engineError === undefined;
  const canStartAnalysis =
    canStart &&
    referenceError === undefined &&
    !!reference &&
    !!engine &&
    selected.length > 0;
  const canStartPlay = canStart && selectedPlayDepth !== undefined;
  async function start(kind: 'analysis' | 'play') {
    if (kind === 'analysis' ? !canStartAnalysis : !canStartPlay) return;
    setStarting(kind);
    setActionError(undefined);
    try {
      const created =
        kind === 'play'
          ? [await startPlayBenchmark(engineId, selectedPlayDepth)]
          : await startBenchmarks(engineId, selected);
      setRuns((current) => [...created, ...current]);
      setQueueActive(true);
      previousActive.current = true;
      setRevision((r) => r + 1);
    } catch (error) {
      setActionError(error);
    } finally {
      setStarting(undefined);
    }
  }
  async function cancel(id: string) {
    if (cancelling) return;
    setCancelling(id);
    setActionError(undefined);
    try {
      await cancelBenchmark(id);
      setRevision((r) => r + 1);
    } catch (error) {
      setActionError(error);
    } finally {
      setCancelling(undefined);
    }
  }
  const time = (run: BenchmarkRun, value: number | null | undefined) =>
    run.status === 'completed' && typeof value === 'number'
      ? `${formatNumber(value, { maximumFractionDigits: 2 })} ms`
      : '—';
  const date = (value: string) => new Date(value).toLocaleString(locale);
  const progress = (run: BenchmarkRun) =>
    run.play_reference
      ? t('playProgress', {
          completed: formatNumber(run.completed_turns),
          total: formatNumber(run.play_reference.total_turns),
        })
      : t('progress', {
          completed: formatNumber(run.completed_games),
          total: formatNumber(run.reference?.total_games ?? 0),
          moves: formatNumber(run.analyzed_moves),
          totalMoves: formatNumber(run.reference?.total_moves ?? 0),
        });
  const statusLabel = (status: string) => {
    switch (status) {
      case 'queued':
      case 'running':
      case 'completed':
      case 'failed':
      case 'cancelled':
        return t(status);
      default:
        return status;
    }
  };
  const error =
    actionError ??
    engineError ??
    historyError ??
    (play ? undefined : referenceError);
  return (
    <div className="benchmark-workspace">
      <header className="benchmark-heading">
        <h1>{t('title')}</h1>
        <p>{t('description')}</p>
      </header>
      {error !== undefined && (
        <div className="error" role="alert">
          {formatError(error)}{' '}
          <button
            type="button"
            onClick={() => {
              setActionError(undefined);
              setConfigurationRevision((r) => r + 1);
              setRevision((r) => r + 1);
            }}
          >
            {t('retry')}
          </button>
        </div>
      )}
      <div className="benchmark-engine-row">
        <label className="benchmark-engine">
          {t('engine')}
          <select
            aria-label={t('engine')}
            value={engineId}
            disabled={Boolean(starting)}
            onChange={(event) => setEngineId(event.target.value)}
          >
            {engines.map((e) => (
              <option key={e.id} value={e.id} disabled={!e.available}>
                {e.name}
              </option>
            ))}
          </select>
        </label>
        {!engine && engines.length > 0 && <p role="status">{t('noEngine')}</p>}
      </div>
      <div
        className="text-navigation benchmark-modes"
        role="group"
        aria-label={t('mode')}
      >
        <button
          type="button"
          aria-pressed={!play}
          onClick={() => setMode('analysis')}
        >
          {t('matchAnalysis')}
        </button>
        <button
          type="button"
          aria-pressed={play}
          onClick={() => setMode('play')}
        >
          {t('playLatency')}
        </button>
      </div>
      {play ? (
        <form
          className="benchmark-configuration"
          onSubmit={(event) => {
            event.preventDefault();
            void start('play');
          }}
        >
          <div className="benchmark-controls">
            <label>
              {t('playPly')}
              <select
                value={selectedPlayDepth ?? ''}
                disabled={Boolean(starting)}
                onChange={(event) => {
                  const value = Number(event.target.value);
                  setPlayDepthsByEngine((current) => ({
                    ...current,
                    [engineId]: value,
                  }));
                }}
              >
                {[0, 1, 2].map((ply) => (
                  <option
                    key={ply}
                    value={ply}
                    disabled={!supportedPlayDepths.includes(ply)}
                  >
                    {formatNumber(ply)}-ply
                  </option>
                ))}
              </select>
            </label>
            <button type="submit" disabled={!canStartPlay}>
              {t(starting === 'play' ? 'starting' : 'startPlay')}
            </button>
          </div>
          {playFallback !== undefined && (
            <p className="benchmark-help" role="status">
              {t('depthFallback', { ply: formatNumber(playFallback) })}
            </p>
          )}
          <p className="benchmark-help">{t('playTiming')}</p>
          {playPlies !== undefined && (
            <p className="benchmark-help">
              {t('playDepth', { ply: formatNumber(playPlies) })}
            </p>
          )}
        </form>
      ) : (
        <form
          className="benchmark-configuration"
          onSubmit={(event) => {
            event.preventDefault();
            void start('analysis');
          }}
        >
          <div className="benchmark-controls">
            <fieldset>
              <legend>{t('plies')}</legend>
              <div className="benchmark-plies">
                {ANALYSIS_PLIES.map((ply) => (
                  <label key={ply}>
                    <input
                      type="checkbox"
                      checked={selected.includes(ply)}
                      disabled={
                        !engine?.plies.includes(ply) || Boolean(starting)
                      }
                      onChange={(event) => {
                        const checked = event.target.checked;
                        setAnalysisDepthsByEngine((current) => ({
                          ...current,
                          [engineId]: checked
                            ? [...selected, ply]
                            : selected.filter((value) => value !== ply),
                        }));
                      }}
                    />
                    {formatNumber(ply)}-ply
                  </label>
                ))}
              </div>
            </fieldset>
            <button type="submit" disabled={!canStartAnalysis}>
              {t(starting === 'analysis' ? 'starting' : 'start')}
            </button>
          </div>
          {analysisFallback !== undefined && (
            <p className="benchmark-help" role="status">
              {t('depthFallback', { ply: formatNumber(analysisFallback) })}
            </p>
          )}
          <p className="benchmark-help">{t('serial')}</p>
        </form>
      )}
      {queueActive && (
        <p className="benchmark-help" role="status">
          {t('engineBusy')}
        </p>
      )}
      {active.length > 0 && (
        <section className="benchmark-active" aria-label={t('active')}>
          <h2>{t('active')}</h2>
          {active.map((run) => (
            <article key={run.id}>
              <div>
                <strong>
                  {t(run.play_reference ? 'playLatency' : 'matchAnalysis')} ·{' '}
                  {run.engine_name} · {formatNumber(run.plies)}-ply ·{' '}
                  {statusLabel(run.status)}
                </strong>
                <p>{progress(run)}</p>
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
                  aria-label={progress(run)}
                />
              </div>
              <button
                type="button"
                disabled={Boolean(cancelling)}
                onClick={() => void cancel(run.id)}
              >
                {t(cancelling === run.id ? 'cancelling' : 'cancel')}
              </button>
            </article>
          ))}
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
                  points: formatNumber(reference.match_length),
                  games: formatNumber(reference.total_games),
                  moves: formatNumber(reference.total_moves),
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
                          seconds: formatNumber(
                            reference.protocol.game_timeout_seconds,
                          ),
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
      <section className="benchmark-history">
        <h2>{t('history')}</h2>
        {loading ? (
          <p role="status">{t('loading')}</p>
        ) : modeRuns.length === 0 ? (
          <p>{t('empty')}</p>
        ) : (
          <div className="benchmark-table-scroll">
            <table>
              <thead>
                <tr>
                  <th scope="col">{t('date')}</th>
                  <th scope="col">{t('engineColumn')}</th>
                  <th scope="col">{t('depth')}</th>
                  {!play && (
                    <th scope="col" className="benchmark-number">
                      {t('total')}
                    </th>
                  )}
                  <th scope="col" className="benchmark-number">
                    {t(play ? 'playAverage' : 'average')}
                  </th>
                  <th scope="col">{t('status')}</th>
                  <th scope="col">{t('details')}</th>
                </tr>
              </thead>
              <tbody>
                {modeRuns.map((run) => (
                  <tr key={run.id}>
                    <td>
                      <time dateTime={run.created_at}>
                        {date(run.created_at)}
                      </time>
                    </td>
                    <td>{run.engine_name}</td>
                    <td>{formatNumber(run.plies)}-ply</td>
                    {!play && (
                      <td
                        className="benchmark-number"
                        data-benchmark-time
                        title={
                          run.status === 'completed' &&
                          typeof run.total_elapsed_ms === 'number' &&
                          run.total_elapsed_ms >= 1000
                            ? time(run, run.total_elapsed_ms)
                            : undefined
                        }
                      >
                        {run.status === 'completed' &&
                        typeof run.total_elapsed_ms === 'number'
                          ? formatBenchmarkDuration(
                              run.total_elapsed_ms,
                              formatNumber,
                            )
                          : '—'}
                      </td>
                    )}
                    <td
                      className={
                        play
                          ? 'benchmark-number benchmark-latency'
                          : 'benchmark-number'
                      }
                      data-benchmark-time
                    >
                      {time(
                        run,
                        play ? run.average_turn_ms : run.average_move_ms,
                      )}
                    </td>
                    <td>
                      <span
                        className={`benchmark-status benchmark-status-${run.status}`}
                      >
                        {statusLabel(run.status)}
                      </span>
                      {run.status !== 'completed' && (
                        <small>{progress(run)}</small>
                      )}
                      {run.error && (
                        <small className="benchmark-run-error">
                          {run.error}
                        </small>
                      )}
                    </td>
                    <td>
                      <BenchmarkRunDetails run={run} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}

function BenchmarkRunDetails({ run }: { run: BenchmarkRun }) {
  const t = useBenchmarkText();
  const { formatNumber } = useI18n();
  const reference = run.play_reference ?? run.reference;

  return (
    <details className="benchmark-row-details">
      <summary>{t('details')}</summary>
      <div>
        <strong>{run.machine.name}</strong>
        <small>
          {run.machine.os} · {run.machine.architecture}
        </small>
        <small>
          {[
            run.machine.cpu_model,
            run.machine.logical_cpus == null
              ? undefined
              : t('cpus', { count: formatNumber(run.machine.logical_cpus) }),
            run.machine.memory_bytes == null
              ? undefined
              : `${formatNumber(run.machine.memory_bytes / 2 ** 30, { maximumFractionDigits: 1 })} GiB`,
          ]
            .filter(Boolean)
            .join(' · ')}
        </small>
        <small>
          <EngineLabel details={run.engine_version} />
        </small>
        <BenchmarkInitialization milliseconds={run.startup_elapsed_ms} />
        <small>
          <code>{run.engine_path}</code>
        </small>
        {reference && (
          <>
            {run.reference && (
              <small>
                {run.reference.name} ·{' '}
                {t('referenceVersion', {
                  version: formatNumber(run.reference.version),
                })}
              </small>
            )}
            <small>
              <code>{reference.id}</code>
            </small>
            <small>
              <code>{run.play_reference?.protocol_id ?? run.protocol?.id}</code>
            </small>
            <small>
              SHA-256 <code>{reference.sha256}</code>
            </small>
          </>
        )}
      </div>
    </details>
  );
}
