import { useEffect, useRef, useState, type ReactNode } from 'react';
import { request, type Match } from './api';
import type { LibraryMatch } from './library-api';
import { useI18n } from './i18n';
import { AppError, type Locale } from './i18n-core';
import { useAnalysisMatchHeaderText } from './analysis-match-header-messages';
import { hasMatchSource } from './match-source';
import './analysis-match-header.css';

interface HeaderState {
  identity: string;
  loading: boolean;
  metadata?: LibraryMatch;
  error?: unknown;
}

export default function AnalysisMatchHeader({
  match,
  metadataRevision = 0,
  analysisRevision,
}: {
  match: Match;
  metadataRevision?: number;
  analysisRevision?: string;
}) {
  const [retry, setRetry] = useState(0);
  const identity = JSON.stringify([
    match.id,
    metadataRevision,
    analysisRevision,
    retry,
  ]);
  const current = useRef(identity);
  current.current = identity;
  const [state, setState] = useState<HeaderState>({ identity, loading: true });

  useEffect(() => {
    const abort = new AbortController();
    setState({ identity, loading: true });
    void request<LibraryMatch>(
      `/matches/${encodeURIComponent(match.id)}/metadata`,
      { signal: abort.signal },
    )
      .then((metadata) => {
        if (metadata.id !== match.id)
          throw new AppError('error.invalidResponse');
        if (!abort.signal.aborted && current.current === identity)
          setState({ identity, loading: false, metadata });
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted && current.current === identity)
          setState({ identity, loading: false, error });
      });
    return () => abort.abort();
  }, [identity, match.id]);

  // Navigation or a metadata/analysis revision may render before effect cleanup.
  const status = state.identity === identity ? state : { loading: true };
  return (
    <AnalysisMatchHeaderView
      match={match}
      metadata={'metadata' in status ? status.metadata : undefined}
      loading={status.loading}
      error={'error' in status ? status.error : undefined}
      onRetry={() => setRetry((value) => value + 1)}
    />
  );
}

function playedDate(value: string | null, locale: Locale) {
  if (!value || !/^\d{4}-\d{2}-\d{2}$/u.test(value)) return 'n/a';
  const date = new Date(`${value}T12:00:00Z`);
  if (
    !Number.isFinite(date.getTime()) ||
    date.toISOString().slice(0, 10) !== value
  )
    return 'n/a';
  return new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeZone: 'UTC',
  }).format(date);
}

export function AnalysisMatchHeaderView({
  match,
  metadata,
  loading,
  error,
  onRetry,
}: {
  match: Match;
  metadata?: LibraryMatch;
  loading: boolean;
  error?: unknown;
  onRetry: () => void;
}) {
  const { t, locale, formatNumber, formatError } = useI18n();
  const h = useAnalysisMatchHeaderText();
  const details = metadata?.id === match.id ? metadata : undefined;
  const sourceAvailable = hasMatchSource(match) && hasMatchSource(details);
  const players = details ?? match;
  const names = `${players.white} ${t('common.versus')} ${players.black}`;
  const pending = '—';
  const pr = (value: number | null) =>
    value == null || !Number.isFinite(value)
      ? 'n/a'
      : formatNumber(value, {
          minimumFractionDigits: 2,
          maximumFractionDigits: 2,
        });
  const result = details
    ? details.white_score != null && details.black_score != null
      ? `${formatNumber(details.white_score)}–${formatNumber(details.black_score)}`
      : 'n/a'
    : pending;
  const scoreTitle =
    details?.result_state === 'completed'
      ? h('finalScore')
      : details?.result_state === 'ongoing'
        ? h('ongoing')
        : undefined;
  const field = (
    key: string,
    label: string,
    value: ReactNode,
    title?: string,
  ) => (
    <div
      className={`analysis-match-field analysis-match-${key}`}
      data-match-field={key}
    >
      <dt>{label}</dt>
      <dd title={title}>{value}</dd>
    </div>
  );
  return (
    <div
      className="analysis-heading analysis-match-header"
      data-match-id={match.id}
      aria-busy={loading}
    >
      <h1 title={names}>
        {players.white} <span>{t('common.versus')}</span> {players.black}
      </h1>
      <dl className="analysis-match-metadata">
        {field(
          'id',
          h('id'),
          details ? formatNumber(details.match_no) : pending,
        )}
        {field(
          'length',
          h('length'),
          players.match_length
            ? formatNumber(players.match_length)
            : t('common.moneyGame'),
        )}
        {field(
          'result',
          h('result'),
          <>
            {result}
            {details?.result_state === 'ongoing' && (
              <small>{h('ongoing')}</small>
            )}
          </>,
          scoreTitle,
        )}
        {field(
          'date',
          h('played'),
          details ? playedDate(details.played_on, locale) : pending,
        )}
        {field(
          'event',
          h('event'),
          details ? details.tournament?.trim() || 'n/a' : pending,
          details?.tournament ?? undefined,
        )}
        {field(
          'pr-white',
          h('whitePr'),
          sourceAvailable ? (details ? pr(details.white_pr) : pending) : 'n/a',
          players.white,
        )}
        {field(
          'pr-black',
          h('blackPr'),
          sourceAvailable ? (details ? pr(details.black_pr) : pending) : 'n/a',
          players.black,
        )}
      </dl>
      {error !== undefined && (
        <div className="analysis-match-error" role="alert">
          <span title={formatError(error)}>{h('loadError')}</span>
          <button type="button" onClick={onRetry}>
            {h('retry')}
          </button>
        </div>
      )}
    </div>
  );
}
