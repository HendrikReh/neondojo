import ActionButton from './ActionButton';
import { useEffect, useId, useRef, useState } from 'react';
import { engineDisplayName } from './engine-version';
import { depthLabel } from './analysis-depth';
import type { Match } from './api';
import { formatBenchmarkDuration } from './benchmark-duration';
import EngineLabel from './EngineLabel';
import options from '../../shared/configuration-options.json' with { type: 'json' };
import {
  cancelSeries,
  createSeries,
  listSeries,
  loadSeries,
  seriesLibraryMatch,
  seriesSgf,
  type CreateSeries,
  type Series,
  type SeriesDetails,
  type SeriesPage,
  type SideCostSummary,
} from './engine-match-api';
import {
  engineMatchText,
  type EngineMatchLabel,
} from './engine-match-messages';
import { useI18n } from './i18n';
import LibraryDialog from './LibraryDialog';
import { loadEngines, type EngineInfo } from './settings-api';
import './engine-match.css';

type Competitor = CreateSeries['a'];
type Mode = NonNullable<CreateSeries['mode']>;
type Format = NonNullable<CreateSeries['format']>;
type Text = (key: EngineMatchLabel) => string;
type SeriesDraft = {
  a: Competitor;
  b: Competitor;
  format: Format;
  mode: Mode;
  length: string;
  blockGames: string;
  count: string;
  seed: string;
};

const initialDraft: SeriesDraft = {
  a: { engine_id: 'gnubg', plies: 2 },
  b: { engine_id: 'gnubg', plies: 1 },
  format: 'match',
  mode: 'paired',
  length: '5',
  blockGames: '100',
  count: '1',
  seed: '',
};
const MAX_BLOCK_GAMES = 1000;
const isMoney = (config: { format?: Format }) =>
  config.format === 'money_cubeless';
/** Label for one stored unit (match or block), or a pair of them. */
const unitLabel = (money: boolean, paired: boolean): EngineMatchLabel =>
  money ? (paired ? 'blockPairs' : 'blocks') : paired ? 'pairs' : 'matches';

const engineName = engineDisplayName;

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
const duelDepths = (id: string, engines: EngineInfo[]) =>
  id === 'gnubg' || id === 'bgsage'
    ? options.engine_profiles[id].duel.filter(
        (depth) =>
          id !== 'bgsage' ||
          depth !== 1 ||
          engines.find((e) => e.id === id)?.plies.includes(1),
      )
    : [];

const validNumber = (value: string, max: number) =>
  Number.isInteger(Number(value)) && Number(value) >= 1 && Number(value) <= max;
const progressLabel = (
  run: Series,
  text: Text,
  number: (value: number) => string,
) =>
  `${number(run.completed_matches)} / ${number(run.total_matches)} ${text(isMoney(run.configuration) ? 'blocks' : 'matches')}`;

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
  const depths = duelDepths(value.engine_id, engines);
  return (
    <fieldset>
      <legend>{text(side)}</legend>
      <select
        aria-label={text(side)}
        value={value.engine_id}
        onChange={(event) => {
          const id = event.target.value;
          onChange({ engine_id: id, plies: 2 });
        }}
      >
        {!engines.some((engine) => engine.id === value.engine_id) && (
          <option value={value.engine_id} disabled>
            {engineName(value.engine_id)}
          </option>
        )}
        {engines.map((engine) => (
          <option
            key={engine.id}
            value={engine.id}
            disabled={!engine.available}
          >
            {engine.name}
            {!engine.available && ` · ${text('unavailable')}`}
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
          {!depths.includes(value.plies) && (
            <option value={value.plies} disabled>
              {number(value.plies)}-ply · {text('unavailable')}
            </option>
          )}
          {depths.map((depth) => (
            <option value={depth} key={depth}>
              {depthLabel(depth, value.engine_id)}
            </option>
          ))}
        </select>
      </label>
    </fieldset>
  );
}

export default function EngineMatchWorkspace({
  visible,
  onOpenMatch,
}: {
  visible: boolean;
  onOpenMatch: (match: Match) => void;
}) {
  const { locale, formatNumber, formatError } = useI18n();
  const text: Text = (key) => engineMatchText(locale, key);
  const seriesHeading = useId();
  const resultsHeading = useId();
  const newSeriesButton = useRef<HTMLButtonElement>(null);
  const setupTrigger = useRef<HTMLButtonElement>(null);
  const [draft, setDraft] = useState(initialDraft);
  const [setupOpen, setSetupOpen] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [setupError, setSetupError] = useState<unknown>();
  const [engines, setEngines] = useState<EngineInfo[]>();
  const [recent, setRecent] = useState<SeriesPage>();
  const [older, setOlder] = useState<{ anchor: string; pages: SeriesPage[] }>();
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [selection, setSelection] = useState('');
  const [offset, setOffset] = useState(0);
  const [resultPage, setResultPage] = useState<{
    offset: number;
    details: SeriesDetails;
  }>();
  const [action, setAction] = useState(false);
  const [opening, setOpening] = useState<number>();
  // Only the latest open request may navigate, and only while this view is shown.
  const openRequest = useRef(0);
  const [error, setError] = useState<unknown>();
  const [loadError, setLoadError] = useState<unknown>();
  const [engineError, setEngineError] = useState<unknown>();
  const [refresh, setRefresh] = useState(0);
  const [retry, setRetry] = useState(0);
  const id = selection || recent?.runs[0]?.id;

  useEffect(() => {
    if (setupOpen || !setupTrigger.current) return;
    const trigger = setupTrigger.current.isConnected
      ? setupTrigger.current
      : newSeriesButton.current;
    setupTrigger.current = null;
    if (visible) trigger?.focus({ preventScroll: true });
  }, [setupOpen, visible]);

  useEffect(() => {
    if (visible) return;
    openRequest.current += 1;
    setOpening(undefined);
  }, [visible]);

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
        const [list, selected] = await Promise.all([
          listSeries(abort.signal),
          id ? loadSeries(id, offset, abort.signal) : undefined,
        ]);
        if (!abort.signal.aborted) {
          setRecent(list);
          if (selected) setResultPage({ offset, details: selected });
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
  const { a, b, format, mode, length, blockGames, count, seed } = draft;
  const money = format === 'money_cubeless';
  const selectable = (side: Competitor) =>
    available(side.engine_id) &&
    engines &&
    duelDepths(side.engine_id, engines).includes(side.plies);
  const canStart =
    !!available('gnubg') &&
    !!selectable(a) &&
    !!selectable(b) &&
    (money
      ? validNumber(blockGames, MAX_BLOCK_GAMES)
      : validNumber(length, 25)) &&
    validNumber(count, 1000);
  const selected =
    resultPage?.details.series.id === id ? resultPage?.details : undefined;
  const displayedError = error ?? loadError;
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

  async function openInAnalysis(runId: string, matchIndex: number) {
    const request = ++openRequest.current;
    setOpening(matchIndex);
    setError(undefined);
    try {
      const match = await seriesLibraryMatch(runId, matchIndex);
      if (request === openRequest.current) onOpenMatch(match);
    } catch (reason) {
      if (request === openRequest.current) setError(reason);
    } finally {
      if (request === openRequest.current) setOpening(undefined);
    }
  }

  async function cancelRun(runId: string) {
    if (action) return;
    setAction(true);
    setError(undefined);
    try {
      await cancelSeries(runId);
      setRefresh((revision) => revision + 1);
    } catch (reason) {
      setError(reason);
    } finally {
      setAction(false);
    }
  }

  const profile = (side: Competitor) =>
    `${engineName(side.engine_id)} ${formatNumber(side.plies)}-ply`;
  const date = new Intl.DateTimeFormat(locale, {
    dateStyle: 'short',
    timeStyle: 'short',
  });
  const rows = [...runs.values()].sort(
    (left, right) => Number(active(right.status)) - Number(active(left.status)),
  );

  const chooseSeries = (runId: string) => {
    if (runId === id) return;
    openRequest.current += 1;
    setOpening(undefined);
    setSelection(runId);
    setResultPage(undefined);
    setOffset(0);
  };

  function openSetup(
    trigger: HTMLButtonElement,
    config?: Series['configuration'],
  ) {
    setupTrigger.current = trigger;
    if (config) {
      setDraft({
        a: config.a,
        b: config.b,
        format: config.format ?? 'match',
        mode: config.mode,
        length: isMoney(config)
          ? initialDraft.length
          : String(config.match_length),
        blockGames: String(config.block_games ?? initialDraft.blockGames),
        count: String(config.count),
        seed: config.seed,
      });
      setAdvancedOpen(true);
    }
    setSetupError(undefined);
    setSetupOpen(true);
  }

  async function startSeries() {
    if (!canStart || action) return;
    setAction(true);
    setSetupError(undefined);
    try {
      const run = await createSeries({
        a,
        b,
        format,
        mode,
        ...(money
          ? { block_games: Number(blockGames) }
          : { match_length: Number(length) }),
        count: Number(count),
        seed: seed.trim() || undefined,
      });
      chooseSeries(run.id);
      setSetupOpen(false);
    } catch (reason) {
      setSetupError(reason);
    } finally {
      setAction(false);
    }
  }

  return (
    <div className="benchmark-workspace engine-match-workspace">
      <h1 className="sr-only">{text('title')}</h1>
      {displayedError !== undefined && (
        <div className="error" role="alert">
          {formatError(displayedError)}{' '}
          <ActionButton
            icon="refresh"
            label={text('retry')}
            compact
            type="button"
            onClick={() => {
              setError(undefined);
              setRetry((revision) => revision + 1);
            }}
          />
        </div>
      )}
      <div className="engine-match-layout">
        <section
          className="engine-match-archive"
          aria-labelledby={seriesHeading}
        >
          <div className="engine-match-archive-heading">
            <h2 id={seriesHeading}>{text('series')}</h2>
            <ActionButton
              icon="add"
              type="button"
              className="primary"
              ref={newSeriesButton}
              label={text('newSeries')}
              onClick={(event) => openSetup(event.currentTarget)}
            />
          </div>
          {!recent ? (
            loadError === undefined && <p role="status">{text('loading')}</p>
          ) : rows.length === 0 ? (
            <p>{text('empty')}</p>
          ) : (
            <ul className="engine-match-series-list">
              {rows.map((run) => (
                <li key={run.id}>
                  <button
                    type="button"
                    aria-current={run.id === id ? 'true' : undefined}
                    onClick={() => chooseSeries(run.id)}
                  >
                    <time dateTime={run.created_at}>
                      {date.format(new Date(run.created_at))}
                    </time>
                    <strong>
                      {profile(run.configuration.a)} /{' '}
                      {profile(run.configuration.b)}
                    </strong>
                    <span
                      className="engine-match-series-status"
                      data-status={run.status}
                    >
                      {statusLabel(run.status, text)}
                      {active(run.status) &&
                        ` · ${progressLabel(run, text, formatNumber)}`}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          {historyCursor && (
            <ActionButton
              icon="expand"
              type="button"
              label={text('olderSeries')}
              disabled={loadingOlder}
              onClick={() => void loadOlder()}
            />
          )}
        </section>
        {id && (
          <section
            className="engine-match-history"
            aria-labelledby={resultsHeading}
          >
            <h2 id={resultsHeading}>{text('results')}</h2>
            {!selected && loadError === undefined && (
              <p role="status">{text('loading')}</p>
            )}
            {selected && (
              <SeriesResults
                key={id}
                details={selected}
                setOffset={(next) => {
                  setSelection(id);
                  setOffset(next);
                }}
                paging={
                  offset !== resultPage?.offset && loadError === undefined
                }
                text={text}
                profile={profile}
                busy={action}
                opening={opening}
                onOpen={(matchIndex) =>
                  void openInAnalysis(selected.series.id, matchIndex)
                }
                onCancel={() => void cancelRun(selected.series.id)}
                onReuse={(trigger) =>
                  openSetup(trigger, selected.series.configuration)
                }
              />
            )}
          </section>
        )}
      </div>
      {visible && setupOpen && (
        <SeriesSetup
          draft={draft}
          setDraft={setDraft}
          engines={engines}
          engineError={engineError}
          retryEngines={() => setRetry((revision) => revision + 1)}
          missingRulesEngine={!!engines && !available('gnubg')}
          advancedOpen={advancedOpen}
          setAdvancedOpen={setAdvancedOpen}
          error={setupError}
          canStart={canStart}
          busy={action}
          close={() => setSetupOpen(false)}
          start={() => void startSeries()}
          text={text}
        />
      )}
    </div>
  );
}

function SeriesSetup({
  draft,
  setDraft,
  engines,
  engineError,
  retryEngines,
  missingRulesEngine,
  advancedOpen,
  setAdvancedOpen,
  error,
  canStart,
  busy,
  close,
  start,
  text,
}: {
  draft: SeriesDraft;
  setDraft: (draft: SeriesDraft) => void;
  engines: EngineInfo[] | undefined;
  engineError: unknown;
  retryEngines: () => void;
  missingRulesEngine: boolean;
  advancedOpen: boolean;
  setAdvancedOpen: (open: boolean) => void;
  error: unknown;
  canStart: boolean;
  busy: boolean;
  close: () => void;
  start: () => void;
  text: Text;
}) {
  const { formatNumber, formatError } = useI18n();
  const advancedBody = useId();
  const formatHint = useId();
  const { a, b, format, mode, length, blockGames, count, seed } = draft;
  const money = format === 'money_cubeless';
  const totalGames =
    validNumber(blockGames, MAX_BLOCK_GAMES) && validNumber(count, 1000)
      ? Number(blockGames) * Number(count) * (mode === 'paired' ? 2 : 1)
      : undefined;

  return (
    <LibraryDialog native title={text('newSeries')} onClose={close} busy={busy}>
      <form
        className="library-dialog-form engine-match-setup"
        onSubmit={(event) => {
          event.preventDefault();
          start();
        }}
      >
        <div className="engine-match-fields">
          <EngineChoice
            side="engineA"
            value={a}
            engines={engines ?? []}
            onChange={(value) => setDraft({ ...draft, a: value })}
            text={text}
            number={formatNumber}
          />
          <EngineChoice
            side="engineB"
            value={b}
            engines={engines ?? []}
            onChange={(value) => setDraft({ ...draft, b: value })}
            text={text}
            number={formatNumber}
          />
        </div>
        <div className="engine-match-rules">
          <label>
            {text('format')}
            <select
              value={format}
              aria-describedby={money ? formatHint : undefined}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  format:
                    event.target.value === 'money_cubeless'
                      ? 'money_cubeless'
                      : 'match',
                })
              }
            >
              <option value="match">{text('formatMatch')}</option>
              <option value="money_cubeless">{text('formatMoney')}</option>
            </select>
          </label>
          <label>
            {text('dice')}
            <select
              value={mode}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  mode: event.target.value === 'random' ? 'random' : 'paired',
                })
              }
            >
              <option value="paired">{text('paired')}</option>
              <option value="random">{text('random')}</option>
            </select>
          </label>
          {money ? (
            <label>
              {text('blockGames')}
              <input
                required
                type="number"
                min="1"
                max={MAX_BLOCK_GAMES}
                value={blockGames}
                onChange={(event) =>
                  setDraft({ ...draft, blockGames: event.target.value })
                }
              />
            </label>
          ) : (
            <label>
              {text('length')}
              <input
                required
                type="number"
                min="1"
                max="25"
                value={length}
                onChange={(event) =>
                  setDraft({ ...draft, length: event.target.value })
                }
              />
            </label>
          )}
          <label>
            {text(unitLabel(money, mode === 'paired'))}
            <input
              required
              type="number"
              min="1"
              max="1000"
              value={count}
              onChange={(event) =>
                setDraft({ ...draft, count: event.target.value })
              }
            />
          </label>
        </div>
        {money && (
          <p id={formatHint} className="engine-match-format-hint">
            {text('formatMoneyHint')}
            {totalGames !== undefined &&
              ` ${text('totalGames')}: ${formatNumber(totalGames)}`}
          </p>
        )}
        <details
          className="engine-match-advanced"
          open={advancedOpen}
          onToggle={(event) => setAdvancedOpen(event.currentTarget.open)}
        >
          <summary aria-controls={advancedBody}>{text('advanced')}</summary>
          <label id={advancedBody}>
            {text('optionalSeed')}
            <input
              value={seed}
              onChange={(event) =>
                setDraft({ ...draft, seed: event.target.value })
              }
              onInvalid={() => setAdvancedOpen(true)}
              spellCheck={false}
              pattern="[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
            />
          </label>
        </details>
        {engineError !== undefined && (
          <div className="error" role="alert">
            {formatError(engineError)}{' '}
            <ActionButton
              icon="refresh"
              label={text('retry')}
              compact
              type="button"
              onClick={retryEngines}
            />
          </div>
        )}
        {!engines && engineError === undefined && (
          <p role="status">{text('loading')}</p>
        )}
        {missingRulesEngine && <p role="status">{text('noEngines')}</p>}
        {error !== undefined && (
          <div className="error">
            <p role="alert">{text('startFailed')}</p>
            <details>
              <summary>{text('details')}</summary>
              <p>{formatError(error)}</p>
            </details>
          </div>
        )}
        <div className="library-dialog-actions">
          <ActionButton
            icon="play"
            type="submit"
            className="primary"
            label={text('start')}
            disabled={!canStart || busy}
          />
        </div>
      </form>
    </LibraryDialog>
  );
}

function SeriesResults({
  details,
  setOffset,
  paging,
  text,
  profile,
  busy,
  opening,
  onOpen,
  onCancel,
  onReuse,
}: {
  details: SeriesDetails;
  setOffset: (offset: number) => void;
  paging: boolean;
  text: Text;
  profile: (side: Competitor) => string;
  busy: boolean;
  /** Index of the match whose library entry is being opened. */
  opening: number | undefined;
  onOpen: (matchIndex: number) => void;
  onCancel: () => void;
  onReuse: (trigger: HTMLButtonElement) => void;
}) {
  const { formatNumber, formatPercent } = useI18n();
  const { series, statistics, matches, next_offset, previous_offset } = details;
  const { configuration: config, engines } = series;
  const paired = config.mode === 'paired';
  const moneyDuel = isMoney(config);
  const money = statistics.money;
  const rate = statistics.win_rate_a;
  const interval = statistics.confidence_interval_a;
  const playing = active(series.status);
  // Stored units (matches or blocks) in the statistics: complete pairs only.
  const counted = paired
    ? statistics.complete_pairs * 2
    : series.completed_matches;
  const signed = (value: number) =>
    formatNumber(value, {
      signDisplay: 'exceptZero',
      minimumFractionDigits: 3,
      maximumFractionDigits: 3,
    });
  const share = (part: number) =>
    money && money.games > 0 ? formatPercent(part / money.games, 1) : '—';
  const costs = [statistics.cost_a ?? null, statistics.cost_b ?? null] as const;
  const duration = (milliseconds: number) =>
    formatBenchmarkDuration(milliseconds, formatNumber);
  const perSide = (format: (cost: SideCostSummary) => string, missing = '—') =>
    costs
      .map(
        (cost, side) => `${side ? 'B' : 'A'} ${cost ? format(cost) : missing}`,
      )
      .join(' · ');
  const stoppedEmpty = !playing && counted === 0 && matches.length === 0;
  const competitors = [
    { side: 'A', profile: config.a, rate, wins: statistics.wins_a },
    {
      side: 'B',
      profile: config.b,
      rate: rate == null ? rate : 1 - rate,
      wins: statistics.wins_b,
    },
  ];
  return (
    <>
      <div className="engine-match-progress" data-active={playing}>
        <p role="status">
          {statusLabel(series.status, text)} ·{' '}
          {progressLabel(series, text, formatNumber)}
        </p>
        {playing && (
          <>
            <progress
              aria-label={text('matches')}
              value={series.completed_matches}
              max={series.total_matches}
            />
            <ActionButton
              icon="cancel"
              type="button"
              label={text('cancel')}
              disabled={busy}
              onClick={onCancel}
            />
          </>
        )}
        <ActionButton
          icon="copy"
          type="button"
          label={text('reuse')}
          aria-haspopup="dialog"
          onClick={(event) => onReuse(event.currentTarget)}
        />
      </div>
      {(series.status === 'failed' || series.error) && (
        <div className="engine-match-failure error">
          {series.status === 'failed' && (
            <p role="alert">
              {text('failedSummary')}
              {series.completed_matches > 0 && ` ${text('partialResults')}`}
            </p>
          )}
          {series.error && (
            <details>
              <summary>{text('details')}</summary>
              <p>{series.error}</p>
            </details>
          )}
        </div>
      )}
      {!stoppedEmpty && (
        <>
          <div className="engine-match-win-rates">
            {competitors.map((competitor) => (
              <dl key={competitor.side}>
                <div className="engine-match-rate">
                  <dt>
                    <strong>
                      {competitor.side} · {profile(competitor.profile)}
                    </strong>
                    <span>{text(moneyDuel ? 'gameWinRate' : 'winRate')}</span>
                  </dt>
                  <dd>
                    {competitor.rate == null
                      ? '—'
                      : formatPercent(competitor.rate, 1)}
                  </dd>
                </div>
                <div className="engine-match-rate-count">
                  <dt>{text(moneyDuel ? 'gamesWon' : 'matchWins')}</dt>
                  <dd>{formatNumber(competitor.wins)}</dd>
                </div>
              </dl>
            ))}
            {rate != null && (
              <div className="engine-match-share" aria-hidden="true">
                <span style={{ inlineSize: `${rate * 100}%` }} />
              </div>
            )}
          </div>
          <dl className="engine-match-statistics">
            <div>
              <dt>{text(moneyDuel ? 'blocksCounted' : 'matchesCounted')}</dt>
              <dd>
                {formatNumber(counted)}
                {money && (
                  <small>
                    {text('games')} {formatNumber(money.games)}
                  </small>
                )}
              </dd>
            </div>
            {paired && (
              <div>
                <dt>{text('completePairs')}</dt>
                <dd>
                  {formatNumber(statistics.complete_pairs)} /{' '}
                  {formatNumber(config.count)}
                </dd>
              </div>
            )}
            {costs.some(Boolean) && (
              <div>
                <dt title={text('cpuPerMoveHint')}>{text('cpuPerMove')}</dt>
                <dd>
                  {perSide((cost) => duration(cost.cpu_ms_per_move))}
                  <small>
                    {text('wallPerMove')}{' '}
                    {perSide((cost) => duration(cost.wall_ms_per_move))}
                  </small>
                  {costs.some((cost) => cost?.measured_matches !== counted) && (
                    <small>
                      {text(moneyDuel ? 'blocksMeasured' : 'costMeasured')}{' '}
                      {perSide(
                        (cost) =>
                          `${formatNumber(cost.measured_matches)} / ${formatNumber(counted)}`,
                        formatNumber(0),
                      )}
                    </small>
                  )}
                </dd>
              </div>
            )}
            {money?.points_per_game_a != null && (
              <div>
                <dt>{text('pointsPerGame')}</dt>
                <dd>
                  {signed(money.points_per_game_a)}
                  {money.confidence_interval && (
                    <small>
                      {text('interval')}:{' '}
                      {money.confidence_interval.map(signed).join(' – ')} ·{' '}
                      {text(
                        money.confidence_interval[0] <= 0 &&
                          money.confidence_interval[1] >= 0
                          ? 'ppgInconclusive'
                          : 'ppgClear',
                      )}
                    </small>
                  )}
                </dd>
              </div>
            )}
            {money && money.games > 0 && (
              <div>
                <dt>{text('gammonShare')}</dt>
                <dd>
                  A {share(money.outcomes_a.gammon)} /{' '}
                  {share(money.outcomes_a.backgammon)} · B{' '}
                  {share(money.outcomes_b.gammon)} /{' '}
                  {share(money.outcomes_b.backgammon)}
                </dd>
              </div>
            )}
            {interval && (
              <div>
                <dt>{text('interval')}</dt>
                <dd>
                  {interval.map((value) => formatPercent(value, 1)).join(' – ')}
                  <small>
                    {text(
                      interval[0] <= 0.5 && interval[1] >= 0.5
                        ? 'intervalInconclusive'
                        : 'intervalClear',
                    )}
                  </small>
                </dd>
              </div>
            )}
          </dl>
        </>
      )}
      <details className="engine-match-settings">
        <summary>{text('seriesSettings')}</summary>
        <p>
          {moneyDuel
            ? `${text('formatMoney')} · ${text(config.mode)} · ${text('blockGames')} ${formatNumber(config.block_games ?? 0)}`
            : `${text(config.mode)} · ${text('length')} ${formatNumber(config.match_length)}`}
        </p>
        <p>
          {text('seed')} <code>{config.seed}</code> · {series.dice_version}
        </p>
        <p>
          {text('engineA')} ·{' '}
          <EngineLabel
            details={engines.a.engine_version ?? profile(config.a)}
            tooltip={
              engines.a.engine_version
                ? `${engines.a.engine_version} · ${formatNumber(config.a.plies)}-ply`
                : undefined
            }
          />
        </p>
        <p>
          {text('engineB')} ·{' '}
          <EngineLabel
            details={engines.b.engine_version ?? profile(config.b)}
            tooltip={
              engines.b.engine_version
                ? `${engines.b.engine_version} · ${formatNumber(config.b.plies)}-ply`
                : undefined
            }
          />
        </p>
      </details>
      {stoppedEmpty ? null : matches.length === 0 ? (
        <p>{text('noMatches')}</p>
      ) : (
        <div className="benchmark-table-scroll" aria-busy={paging}>
          <table>
            <thead>
              <tr>
                <th>{text(unitLabel(moneyDuel, paired))}</th>
                <th>{text('white')}</th>
                <th>{text(moneyDuel ? 'points' : 'score')}</th>
                <th>{text('games')}</th>
                <th>{text('seed')}</th>
                <th>
                  <span className="sr-only">{text('actions')}</span>
                </th>
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
                  <tr
                    key={match.match_index}
                    data-leg={
                      paired ? (isReturn ? 'return' : 'outbound') : undefined
                    }
                  >
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
                    <td className="engine-match-seed">
                      <code>{match.seed}</code>
                    </td>
                    <td className="engine-match-actions">
                      <ActionButton
                        icon="analysis"
                        type="button"
                        label={text('analysis')}
                        tooltip={text('openInAnalysis')}
                        aria-disabled={opening !== undefined || undefined}
                        aria-busy={opening === match.match_index || undefined}
                        onClick={() => {
                          if (opening === undefined) onOpen(match.match_index);
                        }}
                      />
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
        <div className="engine-match-pagination">
          <ActionButton
            icon="back"
            type="button"
            label={text('previous')}
            disabled={previous_offset == null}
            aria-disabled={paging || undefined}
            onClick={() => {
              if (!paging && previous_offset != null)
                setOffset(previous_offset);
            }}
          />
          <ActionButton
            icon="next"
            type="button"
            label={text('next')}
            disabled={next_offset == null}
            aria-disabled={paging || undefined}
            onClick={() => {
              if (!paging && next_offset != null) setOffset(next_offset);
            }}
          />
        </div>
      )}
    </>
  );
}
