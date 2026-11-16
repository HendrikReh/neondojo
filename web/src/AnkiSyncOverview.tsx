import { Fragment, useEffect, useRef, useState } from 'react';
import { DeckDialog } from './DeckLibraryDialogs';
import ActionButton from './ActionButton';
import { useCardPracticeText } from './card-practice-messages';
import { useI18n } from './i18n';
import { AppError } from './i18n-core';
import {
  actOnAnkiSync,
  loadAnkiSyncHistory,
  loadAnkiSyncOverview,
  retryAnkiOpenErrors,
  type AnkiSyncAction,
  type AnkiSyncAttemptResult,
  type AnkiSyncHistoryEntry,
  type LinkedAnkiDeck,
} from './learning-api';

const statusText = {
  checking: 'ankiChecking',
  paused: 'ankiPausedManually',
  unreachable: 'ankiPausedOffline',
  other_profile: 'ankiPausedProfile',
  waiting: 'ankiAutoWaiting',
  synced: 'ankiUpToDate',
  no_matches: 'ankiAutoNoMatches',
  invalid_deck: 'ankiInvalidDeck',
  conflict: 'ankiSyncConflict',
  timeout: 'ankiSyncTimeout',
  anki_error: 'ankiConnectionError',
  internal_error: 'ankiSyncInternalError',
  failed: 'ankiAutoFailed',
} as const satisfies Record<
  LinkedAnkiDeck['status'],
  Parameters<ReturnType<typeof useCardPracticeText>>[0]
>;

type HistoryFilter = 'all' | 'failed' | 'open';

type BulkDeckResult = {
  deck: LinkedAnkiDeck;
  attempt?: AnkiSyncAttemptResult;
  requestError?: string;
};

function BulkResults({ results }: { results: BulkDeckResult[] }) {
  const t = useCardPracticeText();
  const { formatNumber } = useI18n();
  const count = (value?: number) =>
    value === undefined ? '—' : formatNumber(value);
  return (
    <div className="anki-sync-results anki-sync-table-scroll">
      <table className="position-table anki-sync-table">
        <caption>{t('ankiOverviewRetryResults')}</caption>
        <thead>
          <tr>
            <th scope="col">{t('ankiOverviewDeck')}</th>
            <th scope="col">{t('ankiOverviewStatus')}</th>
            <th scope="col">{t('ankiOverviewSent')}</th>
            <th scope="col">{t('ankiOverviewReceived')}</th>
            <th scope="col">{t('ankiOverviewOpenErrors')}</th>
          </tr>
        </thead>
        <tbody>
          {results.map(({ deck, attempt, requestError }) => (
            <tr key={deck.deck_id}>
              <th scope="row">{deck.deck_name}</th>
              <td>
                <span>
                  {t(
                    attempt?.outcome === 'synced'
                      ? 'ankiHistorySucceeded'
                      : attempt?.outcome === 'no_matches'
                        ? 'ankiOverviewRetryNoMatches'
                        : 'ankiOverviewRetryFailed',
                  )}
                </span>
                {(attempt?.error_detail || requestError) && (
                  <small>{attempt?.error_detail || requestError}</small>
                )}
              </td>
              <td>{count(attempt?.sent_reviews)}</td>
              <td>{count(attempt?.received_reviews)}</td>
              <td>{count(attempt?.open_errors)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function SyncHistory({
  deck,
  revision,
  busy,
  filter,
  onFilter,
  onRetry,
}: {
  deck: LinkedAnkiDeck;
  revision: number;
  busy: boolean;
  filter: HistoryFilter;
  onFilter: (filter: HistoryFilter) => void;
  onRetry: (id: string) => Promise<boolean>;
}) {
  const t = useCardPracticeText();
  const { locale, formatNumber, formatError } = useI18n();
  const [rows, setRows] = useState<AnkiSyncHistoryEntry[]>();
  const [error, setError] = useState<unknown>();
  const [retrying, setRetrying] = useState<string>();
  const [retryCompleted, setRetryCompleted] = useState(false);
  useEffect(() => {
    const controller = new AbortController();
    loadAnkiSyncHistory(deck.deck_id, controller.signal)
      .then((next) => {
        if (controller.signal.aborted) return;
        setRows(next);
        setError(undefined);
      })
      .catch((failure: unknown) => {
        if (!controller.signal.aborted) setError(failure);
      });
    return () => controller.abort();
  }, [deck.deck_id, revision]);

  async function retry(id: string) {
    setRetrying(id);
    setRetryCompleted(false);
    setRetryCompleted(await onRetry(id));
    setRetrying(undefined);
  }

  const visibleRows = rows?.filter(
    (row) =>
      filter === 'all' ||
      (row.outcome === 'failed' && (filter === 'failed' || !row.resolved_at)),
  );
  const emptyText = {
    all: 'ankiHistoryEmpty',
    failed: 'ankiHistoryNoFailures',
    open: 'ankiHistoryNoOpenFailures',
  } as const;
  const date = new Intl.DateTimeFormat(locale, {
    dateStyle: 'short',
    timeStyle: 'medium',
  });
  return (
    <div id={`anki-sync-history-${deck.deck_id}`} className="anki-sync-history">
      {!!error && <p role="alert">{formatError(error)}</p>}
      {!rows && !error && <p role="status">{t('ankiHistoryLoading')}</p>}
      {rows && (
        <div className="anki-sync-history-controls">
          <label>
            {t('ankiHistoryFilter')}
            <select
              value={filter}
              onChange={(event) => {
                const value = event.target.value;
                if (value === 'all' || value === 'failed' || value === 'open')
                  onFilter(value);
              }}
            >
              <option value="all">{t('ankiHistoryAll')}</option>
              <option value="failed">{t('ankiHistoryFailedOnly')}</option>
              <option value="open">{t('ankiHistoryOpenOnly')}</option>
            </select>
          </label>
          <span role="status">
            {retryCompleted ? t('ankiHistoryRetryCompleted') : ''}
          </span>
        </div>
      )}
      {visibleRows?.length === 0 && <p>{t(emptyText[filter])}</p>}
      {!!visibleRows?.length && (
        <table className="anki-sync-table">
          <caption>
            {t('ankiHistory')} · {deck.deck_name}
          </caption>
          <thead>
            <tr>
              <th scope="col">{t('ankiHistoryTime')}</th>
              <th scope="col">{t('ankiOverviewReceived')}</th>
              <th scope="col">{t('ankiOverviewSent')}</th>
              <th scope="col">{t('ankiOverviewStatus')}</th>
              <th scope="col">{t('ankiHistoryDetails')}</th>
              <th scope="col">{t('ankiOverviewActions')}</th>
            </tr>
          </thead>
          <tbody>
            {visibleRows.map((row) => {
              const status =
                row.outcome === 'failed'
                  ? (row.problem ?? 'failed')
                  : row.outcome;
              return (
                <tr key={row.id}>
                  <th scope="row">
                    <time dateTime={row.finished_at}>
                      {date.format(new Date(row.finished_at))}
                    </time>
                    <small>
                      {t('ankiAutoProfile', { profile: row.profile })}
                    </small>
                  </th>
                  <td>{formatNumber(row.received_reviews)}</td>
                  <td>{formatNumber(row.sent_reviews)}</td>
                  <td>
                    <span>
                      {t(
                        row.resolved_at
                          ? 'ankiHistoryResolved'
                          : status === 'synced'
                            ? 'ankiHistorySucceeded'
                            : statusText[status],
                      )}
                    </span>
                    {row.resolved_at && (
                      <small>
                        <time dateTime={row.resolved_at}>
                          {date.format(new Date(row.resolved_at))}
                        </time>
                      </small>
                    )}
                  </td>
                  <td>{row.error_detail ?? '—'}</td>
                  <td>
                    {row.outcome === 'failed' && !row.resolved_at && (
                      <ActionButton
                        icon="refresh"
                        label={t('ankiHistoryRetry')}
                        disabled={busy}
                        onClick={() => void retry(row.id)}
                      >
                        {retrying === row.id
                          ? t('syncingAnkiReviews')
                          : t('ankiHistoryRetry')}
                      </ActionButton>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}

export default function AnkiSyncOverview({
  close,
  onSynced,
}: {
  close: () => void;
  onSynced: () => void;
}) {
  const t = useCardPracticeText();
  const { locale, formatNumber, formatError } = useI18n();
  const [decks, setDecks] = useState<LinkedAnkiDeck[]>();
  const [onlyOpenErrors, setOnlyOpenErrors] = useState(false);
  const openErrorsFilter = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<unknown>();
  const [revision, refreshAfterAction] = useState(0);
  const [historyRevision, refreshHistory] = useState(0);
  const [historyDeck, setHistoryDeck] = useState<string>();
  const [historyFilter, setHistoryFilter] = useState<HistoryFilter>('all');
  const [pending, setPending] = useState<{
    deckId: string;
    action: AnkiSyncAction;
  }>();
  const [bulk, setBulk] = useState<{
    total: number;
    results: BulkDeckResult[];
  }>();
  const [actionError, setActionError] = useState<{
    deckId: string;
    message: string;
  }>();

  useEffect(() => {
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    async function refresh() {
      try {
        const next = await loadAnkiSyncOverview(controller.signal);
        if (!controller.signal.aborted) {
          const filter = openErrorsFilter.current;
          if (filter?.checked) {
            const focusedDeck =
              document.activeElement?.closest<HTMLElement>('[data-anki-deck]')
                ?.dataset.ankiDeck;
            if (
              focusedDeck &&
              !next.some(
                (deck) => deck.deck_id === focusedDeck && deck.open_errors > 0,
              )
            )
              filter.focus();
            setHistoryDeck((id) =>
              next.some((deck) => deck.deck_id === id && deck.open_errors > 0)
                ? id
                : undefined,
            );
          }
          setDecks(next);
          setError(undefined);
          refreshHistory((value) => value + 1);
        }
      } catch (failure) {
        if (!controller.signal.aborted) setError(failure);
      } finally {
        if (!controller.signal.aborted)
          timer = setTimeout(() => void refresh(), 10_000);
      }
    }
    void refresh();
    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  }, [revision]);

  function actionErrorMessage(failure: unknown) {
    return failure instanceof AppError && failure.params.status === '503'
      ? t('ankiPausedOffline')
      : formatError(failure);
  }

  async function change(
    deck: LinkedAnkiDeck,
    action: AnkiSyncAction,
    retryOf?: string,
  ) {
    setPending({ deckId: deck.deck_id, action });
    setActionError(undefined);
    try {
      await actOnAnkiSync(deck.deck_id, action, retryOf);
      if (action !== 'sync_now') {
        setDecks((current) =>
          current?.map((row) =>
            row.deck_id === deck.deck_id
              ? {
                  ...row,
                  paused: action === 'pause',
                  status: action === 'pause' ? 'paused' : 'checking',
                }
              : row,
          ),
        );
      }
      return true;
    } catch (failure) {
      setActionError({
        deckId: deck.deck_id,
        message: actionErrorMessage(failure),
      });
      return false;
    } finally {
      if (action === 'sync_now') onSynced();
      refreshAfterAction((value) => value + 1);
      setPending(undefined);
    }
  }

  const affectedDecks = decks?.filter((deck) => deck.open_errors > 0) ?? [];
  const visibleDecks = onlyOpenErrors ? affectedDecks : decks;
  const bulkRunning = !!bulk && bulk.results.length < bulk.total;
  const busy = bulkRunning || !!pending;

  async function retryAllOpenErrors() {
    if (busy || affectedDecks.length === 0) return;
    const total = affectedDecks.length;
    const results: BulkDeckResult[] = [];
    setActionError(undefined);
    setBulk({ total, results: [] });
    for (const deck of affectedDecks) {
      try {
        const attempt = await retryAnkiOpenErrors(deck.deck_id);
        results.push({ deck, attempt });
      } catch (failure) {
        results.push({ deck, requestError: actionErrorMessage(failure) });
      }
      setBulk({ total, results: [...results] });
      refreshAfterAction((value) => value + 1);
    }
    onSynced();
  }

  const succeeded =
    bulk?.results.filter(({ attempt }) => attempt?.outcome === 'synced')
      .length ?? 0;
  const date = new Intl.DateTimeFormat(locale, {
    dateStyle: 'short',
    timeStyle: 'short',
  });
  return (
    <DeckDialog
      title={t('ankiOverview')}
      close={close}
      wide
      busy={bulkRunning}
      className="anki-sync-dialog"
    >
      {!!error && <p role="alert">{formatError(error)}</p>}
      {!decks && !error && <p role="status">{t('ankiOverviewLoading')}</p>}
      {decks?.length === 0 && <p>{t('ankiOverviewEmpty')}</p>}
      {!!decks?.length && (
        <div className="anki-sync-filters">
          <label>
            <input
              ref={openErrorsFilter}
              type="checkbox"
              checked={onlyOpenErrors}
              onChange={(event) => {
                const checked = event.target.checked;
                setOnlyOpenErrors(checked);
                if (
                  checked &&
                  !decks.some(
                    (deck) =>
                      deck.deck_id === historyDeck && deck.open_errors > 0,
                  )
                )
                  setHistoryDeck(undefined);
              }}
            />
            {t('ankiOverviewOnlyDecksWithOpenErrors')}
          </label>
          <ActionButton
            icon="refresh"
            label={t('ankiOverviewRetryAll')}
            disabled={busy || affectedDecks.length === 0}
            onClick={() => void retryAllOpenErrors()}
          />
        </div>
      )}
      {bulk && (
        <p role="status">
          {bulkRunning
            ? t('ankiOverviewRetryAllProgress', {
                completed: formatNumber(bulk.results.length),
                total: formatNumber(bulk.total),
              })
            : t('ankiOverviewRetryAllResult', {
                succeeded,
                failed: bulk.results.length - succeeded,
              })}
        </p>
      )}
      {bulk && !bulkRunning && <BulkResults results={bulk.results} />}
      {!!decks?.length && visibleDecks?.length === 0 && (
        <p role="status">{t('ankiOverviewNoDecksWithOpenErrors')}</p>
      )}
      {!!visibleDecks?.length && (
        <div className="anki-sync-table-scroll">
          <table className="position-table anki-sync-table">
            <caption>{t('ankiOverviewCounts')}</caption>
            <thead>
              <tr>
                <th scope="col">{t('ankiOverviewDeck')}</th>
                <th scope="col">{t('ankiOverviewLastSync')}</th>
                <th scope="col">{t('ankiOverviewReceived')}</th>
                <th scope="col">{t('ankiOverviewSent')}</th>
                <th scope="col">{t('ankiOverviewStatus')}</th>
                <th scope="col">{t('ankiOverviewOpenErrors')}</th>
                <th scope="col">{t('ankiOverviewActions')}</th>
              </tr>
            </thead>
            <tbody>
              {visibleDecks.map((deck) => (
                <Fragment key={deck.deck_id}>
                  <tr data-anki-deck={deck.deck_id}>
                    <th scope="row">
                      {deck.deck_name}
                      <small>
                        {t('ankiAutoProfile', { profile: deck.profile })}
                      </small>
                    </th>
                    <td>
                      {deck.last_synced_at
                        ? date.format(new Date(deck.last_synced_at))
                        : '—'}
                    </td>
                    <td>{formatNumber(deck.received_reviews)}</td>
                    <td>{formatNumber(deck.sent_reviews)}</td>
                    <td>{t(statusText[deck.status])}</td>
                    <td>
                      <button
                        type="button"
                        className="anki-sync-error-count"
                        aria-label={t('ankiOverviewOpenErrorsForDeck', {
                          deck: deck.deck_name,
                          count: formatNumber(deck.open_errors),
                        })}
                        aria-controls={
                          historyDeck === deck.deck_id
                            ? `anki-sync-history-${deck.deck_id}`
                            : undefined
                        }
                        onClick={() => {
                          setHistoryDeck(deck.deck_id);
                          setHistoryFilter('open');
                        }}
                      >
                        {formatNumber(deck.open_errors)}
                      </button>
                    </td>
                    <td>
                      <div className="anki-sync-actions">
                        <ActionButton
                          icon="refresh"
                          label={t('ankiSyncNow')}
                          disabled={busy}
                          onClick={() => void change(deck, 'sync_now')}
                        >
                          {pending?.deckId === deck.deck_id &&
                          pending.action === 'sync_now'
                            ? t('syncingAnkiReviews')
                            : t('ankiSyncNow')}
                        </ActionButton>
                        <ActionButton
                          icon={deck.paused ? 'play' : 'pause'}
                          label={t(
                            deck.paused ? 'ankiResumeSync' : 'ankiPauseSync',
                          )}
                          disabled={busy}
                          onClick={() =>
                            void change(deck, deck.paused ? 'resume' : 'pause')
                          }
                        >
                          {t(deck.paused ? 'ankiResumeSync' : 'ankiPauseSync')}
                        </ActionButton>
                        <ActionButton
                          icon={
                            historyDeck === deck.deck_id ? 'collapse' : 'expand'
                          }
                          label={t('ankiHistory')}
                          aria-expanded={historyDeck === deck.deck_id}
                          aria-controls={
                            historyDeck === deck.deck_id
                              ? `anki-sync-history-${deck.deck_id}`
                              : undefined
                          }
                          onClick={() => {
                            setHistoryFilter('all');
                            setHistoryDeck(
                              historyDeck === deck.deck_id
                                ? undefined
                                : deck.deck_id,
                            );
                          }}
                        />
                      </div>
                      {actionError?.deckId === deck.deck_id && (
                        <p role="alert">{actionError.message}</p>
                      )}
                    </td>
                  </tr>
                  {historyDeck === deck.deck_id && (
                    <tr data-anki-deck={deck.deck_id}>
                      <td colSpan={7}>
                        <SyncHistory
                          deck={deck}
                          revision={historyRevision}
                          busy={busy}
                          filter={historyFilter}
                          onFilter={setHistoryFilter}
                          onRetry={(id) => change(deck, 'sync_now', id)}
                        />
                      </td>
                    </tr>
                  )}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </DeckDialog>
  );
}
