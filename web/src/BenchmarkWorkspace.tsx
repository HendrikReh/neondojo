import { useEffect, useRef, useState } from 'react';
import { ANALYSIS_PLIES } from './analysis-depth';
import { useI18n } from './i18n';
import { loadEngines, type EngineInfo } from './settings-api';
import {
  cancelBenchmark,
  loadBenchmarkReference,
  loadBenchmarks,
  startBenchmarks,
  type BenchmarkReference,
  type BenchmarkRun,
} from './benchmark-api';
import { useBenchmarkText } from './benchmark-messages';
import './benchmark.css';

export default function BenchmarkWorkspace({
  visible,
  defaultPlies,
}: {
  visible: boolean;
  defaultPlies: number;
}) {
  const t = useBenchmarkText();
  const { locale, formatNumber, formatError } = useI18n();
  const [reference, setReference] = useState<BenchmarkReference>();
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [engineId, setEngineId] = useState('gnubg');
  const [plies, setPlies] = useState([defaultPlies]);
  const edited = useRef(false);
  const [runs, setRuns] = useState<BenchmarkRun[]>([]);
  const [loading, setLoading] = useState(true);
  const [configurationError, setConfigurationError] = useState<unknown>();
  const [historyError, setHistoryError] = useState<unknown>();
  const [actionError, setActionError] = useState<unknown>();
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState<string>();
  const [revision, setRevision] = useState(0);
  const [configurationRevision, setConfigurationRevision] = useState(0);
  const [queueActive, setQueueActive] = useState(false);
  const previousActive = useRef(false);
  useEffect(() => {
    if (!edited.current) setPlies([defaultPlies]);
  }, [defaultPlies]);
  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    setConfigurationError(undefined);
    void loadBenchmarkReference(abort.signal)
      .then((nextReference) => {
        if (!abort.signal.aborted) setReference(nextReference);
      })
      .catch((error) => {
        if (!abort.signal.aborted) setConfigurationError(error);
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
        if (!abort.signal.aborted) setConfigurationError(error);
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
  const selected = plies
    .filter((ply) => engine?.plies.includes(ply))
    .sort((a, b) => a - b);
  const active = runs.filter(
    (run) => run.status === 'running' || run.status === 'queued',
  );
  async function start() {
    if (!engine || !selected.length || starting || queueActive) return;
    setStarting(true);
    setActionError(undefined);
    try {
      const created = await startBenchmarks(engine.id, selected);
      setRuns((current) => [...created, ...current]);
      setQueueActive(true);
      previousActive.current = true;
      setRevision((r) => r + 1);
    } catch (error) {
      setActionError(error);
    } finally {
      setStarting(false);
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
  const time = (run: BenchmarkRun, value: number | null) =>
    run.status === 'completed' && value !== null
      ? `${formatNumber(value, { maximumFractionDigits: 2 })} ms`
      : '—';
  const date = (value: string) => new Date(value).toLocaleString(locale);
  const progress = (run: BenchmarkRun) =>
    t('progress', {
      completed: run.completed_games,
      total: run.reference.total_games,
      moves: run.analyzed_moves,
      totalMoves: run.reference.total_moves,
    });
  const error = configurationError ?? historyError ?? actionError;
  return (
    <div className="benchmark-workspace">
      <nav
        className="text-navigation benchmark-navigation"
        aria-label={t('navigation')}
      >
        <button type="button" aria-current="page">
          {t('benchmark')}
        </button>
      </nav>
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
      <div className="benchmark-setup">
        <section className="benchmark-reference" aria-label={t('reference')}>
          <h2>{t('reference')}</h2>
          {reference ? (
            <>
              <h3>{reference.name}</h3>
              <p>
                {t('referenceFacts', {
                  points: reference.match_length,
                  games: reference.total_games,
                  moves: reference.total_moves,
                })}
              </p>
              <p>
                {t('referenceVersion', { version: reference.version })} ·{' '}
                {reference.id}
              </p>
              <p>
                {t('protocol')}: {reference.protocol.id} ·{' '}
                {reference.protocol.game_timeout_seconds === null
                  ? t('noTimeout')
                  : t('timeout', {
                      seconds: reference.protocol.game_timeout_seconds,
                    })}
              </p>
              <details>
                <summary>SHA-256</summary>
                <code>{reference.sha256}</code>
              </details>
            </>
          ) : (
            <p>{t('loading')}</p>
          )}
        </section>
        <form
          className="benchmark-configuration"
          onSubmit={(event) => {
            event.preventDefault();
            void start();
          }}
        >
          <label>
            {t('engine')}
            <select
              aria-label={t('engine')}
              value={engineId}
              onChange={(event) => setEngineId(event.target.value)}
            >
              {engines.map((e) => (
                <option key={e.id} value={e.id} disabled={!e.available}>
                  {e.name}
                  {e.version ? ` · ${e.version}` : ''}
                </option>
              ))}
            </select>
          </label>
          {!engine && engines.length > 0 && <p>{t('noEngine')}</p>}
          <fieldset>
            <legend>{t('plies')}</legend>
            <div className="benchmark-plies">
              {ANALYSIS_PLIES.map((ply) => (
                <label key={ply}>
                  <input
                    type="checkbox"
                    checked={selected.includes(ply)}
                    disabled={!engine?.plies.includes(ply) || starting}
                    onChange={(event) => {
                      edited.current = true;
                      setPlies((current) =>
                        event.target.checked
                          ? [...current, ply]
                          : current.filter((value) => value !== ply),
                      );
                    }}
                  />
                  {ply}-ply
                </label>
              ))}
            </div>
          </fieldset>
          <button
            type="submit"
            disabled={
              !reference ||
              !engine ||
              !selected.length ||
              starting ||
              Boolean(configurationError) ||
              queueActive ||
              loading
            }
          >
            {t(starting ? 'starting' : 'start')}
          </button>
          <p className="benchmark-help">{t('serial')}</p>
        </form>
      </div>
      <p className="benchmark-help benchmark-timing">{t('timing')}</p>
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
                  {run.engine_name} · {run.plies}-ply · {t(run.status)}
                </strong>
                <p>{progress(run)}</p>
                {(run.status === 'queued' || run.started_at === null) && (
                  <p>{t('waiting')}</p>
                )}
                <progress
                  max={run.reference.total_games}
                  value={run.completed_games}
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
      <section className="benchmark-history">
        <h2>{t('history')}</h2>
        {loading ? (
          <p role="status">{t('loading')}</p>
        ) : runs.length === 0 ? (
          <p>{t('empty')}</p>
        ) : (
          <div className="benchmark-table-scroll">
            <table>
              <thead>
                <tr>
                  {(
                    [
                      'date',
                      'machine',
                      'engineVersion',
                      'depth',
                      'reference',
                      'total',
                      'average',
                      'status',
                    ] as const
                  ).map((key) => (
                    <th key={key} scope="col">
                      {t(key)}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {runs.map((run) => (
                  <tr key={run.id}>
                    <td>
                      <time dateTime={run.created_at}>
                        {date(run.created_at)}
                      </time>
                    </td>
                    <td>
                      <strong>{run.machine.name}</strong>
                      <small>
                        {run.machine.os} · {run.machine.architecture}
                      </small>
                      <small>
                        {run.machine.cpu_model ?? '—'} ·{' '}
                        {run.machine.logical_cpus === null
                          ? '—'
                          : t('cpus', { count: run.machine.logical_cpus })}{' '}
                        ·{' '}
                        {run.machine.memory_bytes === null
                          ? '—'
                          : `${formatNumber(run.machine.memory_bytes / 2 ** 30, { maximumFractionDigits: 1 })} GiB`}
                      </small>
                    </td>
                    <td>
                      {run.engine_name}
                      <small>{run.engine_version}</small>
                      <small className="benchmark-path">
                        {run.engine_path}
                      </small>
                    </td>
                    <td>{run.plies}-ply</td>
                    <td>
                      {run.reference.name}
                      <small>
                        {t('referenceVersion', {
                          version: run.reference.version,
                        })}{' '}
                        · {run.protocol.id}
                      </small>
                    </td>
                    <td data-benchmark-time>
                      {time(run, run.total_elapsed_ms)}
                    </td>
                    <td data-benchmark-time>
                      {time(run, run.average_move_ms)}
                    </td>
                    <td>
                      <span
                        className={`benchmark-status benchmark-status-${run.status}`}
                      >
                        {t(run.status)}
                      </span>
                      <small>{progress(run)}</small>
                      {run.error && (
                        <small className="benchmark-run-error">
                          {run.error}
                        </small>
                      )}
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
