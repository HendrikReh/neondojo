import BenchmarkComparison, {
  BenchmarkInitialization,
} from './BenchmarkComparison';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import { useEffect, useMemo, useRef, useState } from 'react';
import ActionButton from './shared/ui/ActionButton';
import EngineLabel from './shared/engine/EngineLabel';
import LibrarySortHeading from './shared/ui/LibrarySortHeading';
import {
  ANALYSIS_PLIES,
  PLAY_PLIES,
  preferredDepth as enginePreferredDepth,
} from './shared/engine/analysis-depth';
import { formatBenchmarkDuration } from './benchmark-duration';
import {
  DEFAULT_BENCHMARK_HISTORY_ORDER,
  nextBenchmarkHistoryOrder,
  sortBenchmarkRuns,
  type BenchmarkHistorySort,
} from './benchmark-history-sort';
import { useI18n } from './shared/i18n/i18n';
import { loadEngines, type EngineInfo } from './settings/settings-api';
import {
  cancelBenchmark,
  deleteBenchmark,
  loadBenchmarkReference,
  loadBenchmarks,
  startBenchmarks,
  startPlayBenchmark,
  type BenchmarkReference,
  type BenchmarkRun,
} from './benchmark-api';
import { useBenchmarkText } from './benchmark-messages';
import './benchmark.css';

function preferredDepth(engineId: string, depths: readonly number[]) {
  const preferred = enginePreferredDepth(engineId);
  return depths.includes(preferred) ? preferred : depths[0];
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
  const [confirmingDelete, setConfirmingDelete] = useState<string>();
  const [deleting, setDeleting] = useState<string>();
  const [historyOrder, setHistoryOrder] = useState(
    DEFAULT_BENCHMARK_HISTORY_ORDER,
  );
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
          // Follow an active queue closely; an idle history only needs to
          // notice runs started elsewhere. Starting a run here re-polls at once.
          timer = setTimeout(
            () => void poll(),
            previousActive.current ? 1000 : 10_000,
          );
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
      ? preferredDepth(engineId, availableAnalysisDepths)
      : undefined;
  const selected =
    analysisFallback === undefined ? compatiblePlies : [analysisFallback];
  const play = mode === 'play';
  const modeRuns = useMemo(
    () => runs.filter((run) => Boolean(run.play_reference) === play),
    [runs, play],
  );
  const historyRuns = useMemo(
    () => sortBenchmarkRuns(modeRuns, historyOrder, play, locale),
    [modeRuns, historyOrder, play, locale],
  );
  const sortHeading = (
    sort: BenchmarkHistorySort,
    label: string,
    className?: string,
  ) => (
    <LibrarySortHeading<BenchmarkHistorySort>
      fields={[{ key: sort, label }]}
      sort={historyOrder.sort}
      direction={historyOrder.direction}
      className={className}
      onSort={(next) =>
        setHistoryOrder((current) => nextBenchmarkHistoryOrder(current, next))
      }
    />
  );
  const supportedPlayDepths = (
    engineId === 'bgsage'
      ? configurationOptions.engine_profiles.bgsage.play
      : configurationOptions.engine_profiles.gnubg.play
  ).filter((ply) => engine?.plies.includes(ply));
  const requestedPlayDepth =
    playDepthsByEngine[engineId] ?? playPlies ?? enginePreferredDepth(engineId);
  const playDepthSupported = supportedPlayDepths.includes(requestedPlayDepth);
  const playFallback = playDepthSupported
    ? undefined
    : preferredDepth(engineId, supportedPlayDepths);
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
  async function remove(id: string) {
    if (deleting) return;
    setDeleting(id);
    setActionError(undefined);
    try {
      await deleteBenchmark(id);
      setRuns((current) => current.filter((run) => run.id !== id));
      setConfirmingDelete(undefined);
      setRevision((r) => r + 1);
    } catch (error) {
      setActionError(error);
      setRevision((r) => r + 1);
    } finally {
      setDeleting(undefined);
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
            onClick={() => {
              setActionError(undefined);
              setConfigurationRevision((r) => r + 1);
              setRevision((r) => r + 1);
            }}
          />
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
            {engines.length === 0 && engineError === undefined && (
              <option value={engineId}>{t('loading')}</option>
            )}
            {engines.map((e) => (
              <option key={e.id} value={e.id} disabled={!e.available}>
                {e.name}
              </option>
            ))}
          </select>
        </label>
        {!engine && engines.length > 0 && <p role="status">{t('noEngine')}</p>}
      </div>
      <div className="benchmark-modes" role="group" aria-label={t('mode')}>
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
                {PLAY_PLIES.map((ply) => (
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
            <ActionButton
              icon="play"
              className="primary"
              type="submit"
              label={t(starting === 'play' ? 'starting' : 'startPlay')}
              disabled={!canStartPlay}
            />
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
            <ActionButton
              icon="play"
              className="primary"
              type="submit"
              label={t(starting === 'analysis' ? 'starting' : 'start')}
              disabled={!canStartAnalysis}
            />
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
              <ActionButton
                icon="cancel"
                type="button"
                label={t(cancelling === run.id ? 'cancelling' : 'cancel')}
                disabled={Boolean(cancelling)}
                onClick={() => void cancel(run.id)}
              />
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
                  {sortHeading('date', t('date'))}
                  {sortHeading('engine', t('engineColumn'))}
                  {sortHeading('plies', t('depth'))}
                  {!play &&
                    sortHeading('total', t('total'), 'benchmark-number')}
                  {sortHeading(
                    'average',
                    t(play ? 'playAverage' : 'average'),
                    'benchmark-number',
                  )}
                  {sortHeading('status', t('status'))}
                  <th scope="col">{t('details')}</th>
                  <th scope="col">
                    <span className="sr-only">{t('actions')}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {historyRuns.map((run) => (
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
                    <td className="benchmark-row-actions">
                      {run.status !== 'queued' &&
                        run.status !== 'running' &&
                        (confirmingDelete === run.id ? (
                          <div
                            className="benchmark-delete-confirm"
                            role="group"
                            aria-label={t('deleteQuestion')}
                            onKeyDown={(event) => {
                              if (event.key === 'Escape' && !deleting) {
                                event.stopPropagation();
                                setConfirmingDelete(undefined);
                              }
                            }}
                          >
                            <span>{t('deleteQuestion')}</span>
                            <ActionButton
                              icon="delete"
                              className="danger"
                              type="button"
                              label={t(
                                deleting === run.id
                                  ? 'deleting'
                                  : 'confirmDelete',
                              )}
                              autoFocus
                              disabled={Boolean(deleting)}
                              onClick={() => void remove(run.id)}
                            />
                            <ActionButton
                              icon="cancel"
                              type="button"
                              label={t('keepRun')}
                              disabled={Boolean(deleting)}
                              onClick={() => setConfirmingDelete(undefined)}
                            />
                          </div>
                        ) : (
                          <ActionButton
                            icon="delete"
                            type="button"
                            label={t('deleteRun')}
                            compact
                            disabled={Boolean(deleting)}
                            onClick={() => setConfirmingDelete(run.id)}
                          />
                        ))}
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
              : t('cpus', { count: run.machine.logical_cpus }),
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
