import ActionButton from '../../shared/ui/ActionButton';
import { useEffect, useId, useRef, useState } from 'react';
import { engineDisplayName } from '../../shared/engine/engine-version';
import type { Match } from '../../shared/http/api';
import {
  cancelSeries,
  createSeries,
  listSeries,
  loadSeries,
  seriesLibraryMatch,
  type Series,
  type SeriesDetails,
  type SeriesPage,
} from './engine-match-api';
import { engineMatchText } from './engine-match-messages';
import EngineSeriesResults from './EngineSeriesResults';
import EngineSeriesSetup from './EngineSeriesSetup';
import {
  duelDepths,
  initialDraft,
  isActive,
  isMoney,
  MAX_BLOCK_GAMES,
  progressLabel,
  statusLabel,
  validNumber,
  type Competitor,
  type Text,
} from './engine-series';
import { useI18n } from '../../shared/i18n/i18n';
import { loadEngines, type EngineInfo } from '../../settings/settings-api';
import './engine-match.css';
import { dateTimeFormat } from '../../shared/i18n/i18n-core';

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
            list.runs.some((run) => isActive(run.status)) ||
            (selected !== undefined && isActive(selected.series.status));
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
    `${engineDisplayName(side.engine_id)} ${formatNumber(side.plies)}-ply`;
  const date = dateTimeFormat(locale, {
    dateStyle: 'short',
    timeStyle: 'short',
  });
  const rows = [...runs.values()].sort(
    (left, right) =>
      Number(isActive(right.status)) - Number(isActive(left.status)),
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
                      {isActive(run.status) &&
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
              <EngineSeriesResults
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
        <EngineSeriesSetup
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
