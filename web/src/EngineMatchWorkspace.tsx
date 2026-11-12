import { useEffect, useState } from 'react';
import options from '../../shared/configuration-options.json' with { type: 'json' };
import {
  cancelSeries,
  createSeries,
  listSeries,
  loadSeries,
  seriesSgf,
  type CreateSeries,
  type Series,
  type SeriesDetails,
  type SeriesPage,
} from './engine-match-api';
import {
  engineMatchText,
  type EngineMatchLabel,
} from './engine-match-messages';
import { useI18n } from './i18n';
import { loadEngines, type EngineInfo } from './settings-api';
import './engine-match.css';

type Competitor = CreateSeries['a'];
type Mode = NonNullable<CreateSeries['mode']>;
type Text = (key: EngineMatchLabel) => string;

const active = (status: string) => status === 'queued' || status === 'running';
const statusLabel = (status: string, text: Text) => {
  switch (status) {
    case 'queued':
    case 'running':
    case 'completed':
    case 'failed':
    case 'cancelled':
      return text(status);
    default:
      return status;
  }
};
const playingDepths = (id: string) =>
  id === 'gnubg' || id === 'bgsage' ? options.engine_profiles[id].play : [];
const validNumber = (value: string, max: number) =>
  Number.isInteger(Number(value)) && Number(value) >= 1 && Number(value) <= max;

function EngineChoice({
  side,
  value,
  engines,
  onChange,
  text,
  number,
}: {
  side: 'engineA' | 'engineB';
  value: Competitor;
  engines: EngineInfo[];
  onChange: (next: Competitor) => void;
  text: Text;
  number: (value: number) => string;
}) {
  return (
    <fieldset>
      <legend>{text(side)}</legend>
      <select
        aria-label={text(side)}
        value={value.engine_id}
        onChange={(event) => {
          const id = event.target.value;
          onChange({ engine_id: id, plies: playingDepths(id).at(-1) ?? 0 });
        }}
      >
        {engines.map((engine) => (
          <option
            key={engine.id}
            value={engine.id}
            disabled={!engine.available}
          >
            {engine.name}
          </option>
        ))}
      </select>
      <label>
        {text('strength')}
        <select
          value={value.plies}
          onChange={(event) =>
            onChange({
              ...value,
              plies: Number(event.target.value),
            })
          }
        >
          {playingDepths(value.engine_id).map((depth) => (
            <option value={depth} key={depth}>
              {number(depth)}-ply
            </option>
          ))}
        </select>
      </label>
    </fieldset>
  );
}

export default function EngineMatchWorkspace({
  visible,
}: {
  visible: boolean;
}) {
  const { locale, formatNumber, formatError } = useI18n();
  const text: Text = (key) => engineMatchText(locale, key);
  const [a, setA] = useState<Competitor>({ engine_id: 'gnubg', plies: 1 });
  const [b, setB] = useState<Competitor>({ engine_id: 'gnubg', plies: 0 });
  const [mode, setMode] = useState<Mode>('paired');
  const [length, setLength] = useState('5');
  const [count, setCount] = useState('1');
  const [seed, setSeed] = useState('');
  const [engines, setEngines] = useState<EngineInfo[]>();
  const [recent, setRecent] = useState<SeriesPage>();
  const [older, setOlder] = useState<{ anchor: string; pages: SeriesPage[] }>();
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [selection, setSelection] = useState('');
  const [offset, setOffset] = useState(0);
  const [details, setDetails] = useState<SeriesDetails>();
  const [action, setAction] = useState(false);
  const [error, setError] = useState<unknown>();
  const [loadError, setLoadError] = useState<unknown>();
  const [engineError, setEngineError] = useState<unknown>();
  const [refresh, setRefresh] = useState(0);
  const [retry, setRetry] = useState(0);
  const id = selection || recent?.runs[0]?.id;

  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    void loadEngines(abort.signal)
      .then((catalog) => {
        if (!abort.signal.aborted) {
          setEngines(catalog);
          setEngineError(undefined);
        }
      })
      .catch((reason: unknown) => {
        if (!abort.signal.aborted) setEngineError(reason);
      });
    return () => abort.abort();
  }, [visible, retry]);

  useEffect(() => {
    if (!visible) return;
    const abort = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      let pollAgain = true;
      try {
        const list = await listSeries(abort.signal);
        const selected = id
          ? await loadSeries(id, offset, abort.signal)
          : undefined;
        if (!abort.signal.aborted) {
          setRecent(list);
          setDetails(selected);
          setLoadError(undefined);
          pollAgain =
            list.runs.some((run) => active(run.status)) ||
            (selected !== undefined && active(selected.series.status));
        }
      } catch (reason) {
        if (!abort.signal.aborted) setLoadError(reason);
      } finally {
        if (!abort.signal.aborted && pollAgain)
          timer = setTimeout(() => void poll(), 1500);
      }
    }
    void poll();
    return () => {
      abort.abort();
      clearTimeout(timer);
    };
  }, [visible, id, offset, refresh, retry]);

  const available = (engineId: string) =>
    engines?.some((engine) => engine.id === engineId && engine.available);
  const canStart =
    available('gnubg') &&
    available(a.engine_id) &&
    available(b.engine_id) &&
    validNumber(length, 25) &&
    validNumber(count, 1000);
  const selected = details?.series.id === id ? details : undefined;
  const displayedError = error ?? loadError ?? engineError;
  const olderPages =
    older?.anchor === recent?.next_cursor ? (older?.pages ?? []) : [];
  const runs = new Map(recent?.runs.map((run) => [run.id, run]));
  for (const page of olderPages) {
    for (const run of page.runs) {
      if (!runs.has(run.id)) runs.set(run.id, run);
    }
  }
  if (selected) runs.set(selected.series.id, selected.series);
  const historyCursor = (olderPages.at(-1) ?? recent)?.next_cursor;

  async function loadOlder() {
    const anchor = recent?.next_cursor;
    if (!anchor || !historyCursor || loadingOlder) return;
    setLoadingOlder(true);
    setError(undefined);
    try {
      const page = await listSeries(undefined, historyCursor);
      setOlder((current) => ({
        anchor,
        pages: current?.anchor === anchor ? [...current.pages, page] : [page],
      }));
    } catch (reason) {
      setError(reason);
    } finally {
      setLoadingOlder(false);
    }
  }

  async function mutate(operation: () => Promise<Series>) {
    setAction(true);
    setError(undefined);
    try {
      const series = await operation();
      setRefresh((revision) => revision + 1);
      return series;
    } catch (reason) {
      setError(reason);
    } finally {
      setAction(false);
    }
  }

  const profile = (side: Competitor) =>
    `${side.engine_id === 'gnubg' ? 'GNUbg' : 'Open Sage'} ${formatNumber(side.plies)}-ply`;
  const runLabel = (run: Series) =>
    `${new Intl.DateTimeFormat(locale, { dateStyle: 'short', timeStyle: 'medium' }).format(new Date(run.created_at))} · ${profile(run.configuration.a)} / ${profile(run.configuration.b)} · ${statusLabel(run.status, text)}`;

  return (
    <div className="benchmark-workspace engine-match-workspace">
      <header className="benchmark-heading">
        <h1>{text('title')}</h1>
      </header>
      {displayedError !== undefined && (
        <div className="error" role="alert">
          {formatError(displayedError)}{' '}
          <button
            type="button"
            onClick={() => {
              setError(undefined);
              setRetry((revision) => revision + 1);
            }}
          >
            {text('retry')}
          </button>
        </div>
      )}
      <form
        className="engine-match-setup"
        onSubmit={(event) => {
          event.preventDefault();
          if (!canStart || action) return;
          void mutate(() =>
            createSeries({
              a,
              b,
              mode,
              match_length: Number(length),
              count: Number(count),
              seed: seed.trim() || undefined,
            }),
          ).then((series) => {
            if (series) {
              setSelection(series.id);
              setOffset(0);
            }
          });
        }}
      >
        <div className="engine-match-fields">
          <EngineChoice
            side="engineA"
            value={a}
            engines={engines ?? []}
            onChange={setA}
            text={text}
            number={formatNumber}
          />
          <EngineChoice
            side="engineB"
            value={b}
            engines={engines ?? []}
            onChange={setB}
            text={text}
            number={formatNumber}
          />
          <label>
            {text('dice')}
            <select
              value={mode}
              onChange={(event) =>
                setMode(event.target.value === 'random' ? 'random' : 'paired')
              }
            >
              <option value="paired">{text('paired')}</option>
              <option value="random">{text('random')}</option>
            </select>
          </label>
          <label>
            {text('length')}
            <input
              required
              type="number"
              min="1"
              max="25"
              value={length}
              onChange={(event) => setLength(event.target.value)}
            />
          </label>
          <label>
            {text(mode === 'paired' ? 'pairs' : 'matches')}
            <input
              required
              type="number"
              min="1"
              max="1000"
              value={count}
              onChange={(event) => setCount(event.target.value)}
            />
          </label>
          <label>
            {text('optionalSeed')}
            <input
              value={seed}
              onChange={(event) => setSeed(event.target.value)}
              spellCheck={false}
              pattern="[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
            />
          </label>
        </div>
        <button type="submit" disabled={!canStart || action}>
          {text('start')}
        </button>
        {engines && !available('gnubg') && (
          <p role="status">{text('noEngines')}</p>
        )}
      </form>

      <section className="engine-match-history" aria-label={text('series')}>
        <h2>{text('series')}</h2>
        {!recent ? (
          <p>{text('loading')}</p>
        ) : runs.size === 0 ? (
          <p>{text('empty')}</p>
        ) : (
          <select
            aria-label={text('series')}
            value={id}
            onChange={(event) => {
              setSelection(event.target.value);
              setDetails(undefined);
              setOffset(0);
            }}
          >
            {Array.from(runs.values(), (run) => (
              <option key={run.id} value={run.id}>
                {runLabel(run)}
              </option>
            ))}
          </select>
        )}
        {historyCursor && (
          <button
            type="button"
            disabled={loadingOlder}
            onClick={() => void loadOlder()}
          >
            {text('olderSeries')}
          </button>
        )}
        {id && !selected && <p>{text('loading')}</p>}
        {selected && (
          <SeriesResults
            details={selected}
            setOffset={(next) => {
              setDetails(undefined);
              setOffset(next);
            }}
            text={text}
            profile={profile}
            busy={action}
            onCancel={() => {
              void mutate(() => cancelSeries(selected.series.id));
            }}
            onReuse={() => {
              const config = selected.series.configuration;
              setA(config.a);
              setB(config.b);
              setMode(config.mode);
              setLength(String(config.match_length));
              setCount(String(config.count));
              setSeed(config.seed);
            }}
          />
        )}
      </section>
    </div>
  );
}

function SeriesResults({
  details,
  setOffset,
  text,
  profile,
  busy,
  onCancel,
  onReuse,
}: {
  details: SeriesDetails;
  setOffset: (offset: number) => void;
  text: Text;
  profile: (side: Competitor) => string;
  busy: boolean;
  onCancel: () => void;
  onReuse: () => void;
}) {
  const { formatNumber, formatPercent } = useI18n();
  const { series, statistics, matches, next_offset, previous_offset } = details;
  const { configuration: config, engines } = series;
  const paired = config.mode === 'paired';
  const rate = statistics.win_rate_a;
  const interval = statistics.confidence_interval_a;
  const progress = `${formatNumber(series.completed_matches)} / ${formatNumber(series.total_matches)}`;
  return (
    <>
      <div className="engine-match-progress">
        <p role="status">
          {statusLabel(series.status, text)} · {progress} {text('matches')}
        </p>
        {active(series.status) && (
          <>
            <progress
              aria-label={text('matches')}
              value={series.completed_matches}
              max={series.total_matches}
            />
            <button type="button" disabled={busy} onClick={onCancel}>
              {text('cancel')}
            </button>
          </>
        )}
        <button type="button" onClick={onReuse}>
          {text('reuse')}
        </button>
      </div>
      {series.error && (
        <p className="error" role="alert">
          {series.error}
        </p>
      )}
      <p className="benchmark-help">
        A · {profile(config.a)} · {engines.a.engine_version ?? '—'}
        <br />B · {profile(config.b)} · {engines.b.engine_version ?? '—'}
        <br />
        {text(config.mode)} · {text('length')}{' '}
        {formatNumber(config.match_length)} · {text('seed')}{' '}
        <code>{config.seed}</code> · {series.dice_version}
      </p>
      <dl className="engine-match-statistics">
        {paired && (
          <div>
            <dt>{text('completePairs')}</dt>
            <dd>
              {formatNumber(statistics.complete_pairs)} /{' '}
              {formatNumber(config.count)}
            </dd>
          </div>
        )}
        <div>
          <dt>{text('winRate')}</dt>
          <dd>
            {rate == null ? (
              '—'
            ) : (
              <>
                A {formatPercent(rate, 1)} ({formatNumber(statistics.wins_a)}) ·
                B {formatPercent(1 - rate, 1)} (
                {formatNumber(statistics.wins_b)})
              </>
            )}
          </dd>
        </div>
        {interval && (
          <div>
            <dt>{text('interval')}</dt>
            <dd>
              {interval.map((value) => formatPercent(value, 1)).join(' – ')}
            </dd>
          </div>
        )}
      </dl>
      {matches.length === 0 ? (
        <p>{text('noMatches')}</p>
      ) : (
        <div className="benchmark-table-scroll">
          <table>
            <thead>
              <tr>
                <th>{text(paired ? 'pairs' : 'matches')}</th>
                <th>{text('white')}</th>
                <th>{text('score')}</th>
                <th>{text('games')}</th>
                <th>{text('seed')}</th>
                <th>SGF</th>
              </tr>
            </thead>
            <tbody>
              {matches.map((match) => {
                const isReturn = paired && match.match_index % 2 === 1;
                const incomplete =
                  paired &&
                  !isReturn &&
                  match.match_index + 1 === series.completed_matches;
                return (
                  <tr key={match.match_index}>
                    <th scope="row">
                      {formatNumber(
                        paired
                          ? Math.floor(match.match_index / 2) + 1
                          : match.match_index + 1,
                      )}
                      {paired && ` · ${text(isReturn ? 'return' : 'outbound')}`}
                      {incomplete && <small>{text('incompletePair')}</small>}
                    </th>
                    <td>{isReturn ? 'B' : 'A'}</td>
                    <td>
                      {formatNumber(match.score_a)}–
                      {formatNumber(match.score_b)}
                    </td>
                    <td>{formatNumber(match.games)}</td>
                    <td>
                      <code>{match.seed}</code>
                    </td>
                    <td>
                      <a
                        href={seriesSgf(series.id, match.match_index)}
                        download
                      >
                        SGF
                      </a>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {(previous_offset != null || next_offset != null) && (
        <div className="engine-match-progress">
          <button
            type="button"
            disabled={previous_offset == null}
            onClick={() => {
              if (previous_offset != null) setOffset(previous_offset);
            }}
          >
            {text('previous')}
          </button>
          <button
            type="button"
            disabled={next_offset == null}
            onClick={() => {
              if (next_offset != null) setOffset(next_offset);
            }}
          >
            {text('next')}
          </button>
        </div>
      )}
    </>
  );
}
