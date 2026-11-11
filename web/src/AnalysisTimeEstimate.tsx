import { useEffect, useState } from 'react';
import { useOptionalConfiguration } from './ConfigurationProvider';
import { useI18n } from './i18n';
import {
  loadAnalysisTimeEstimate,
  type AnalysisTimeEstimateResult,
} from './analysis-time-estimate-api';
import {
  analysisEstimateText,
  formatAnalysisDuration,
} from './analysis-time-estimate-messages';
import './analysis-time-estimate.css';

export default function AnalysisTimeEstimate({
  matchId,
  gameNo,
  scoped,
  plies,
  configured,
  installationKey = '',
}: {
  matchId: string;
  gameNo?: number;
  scoped: boolean;
  plies: number;
  configured: boolean;
  installationKey?: string;
}) {
  const profile = useOptionalConfiguration();
  const { locale, formatNumber } = useI18n();
  const text = (
    key: Parameters<typeof analysisEstimateText>[1],
    values?: Parameters<typeof analysisEstimateText>[2],
  ) => analysisEstimateText(locale, key, values);
  const current = profile?.current?.analysis;
  const confirmed = profile?.confirmed?.current_configuration.analysis;
  const engineId = current?.engine ?? 'gnubg';
  // `ready` remains true during the save debounce. Compare the actual engine
  // inputs with the server snapshot, without flushing or writing on a read.
  const pending =
    !!profile &&
    (!profile.ready ||
      profile.busy ||
      !current ||
      !confirmed ||
      current.engine !== confirmed.engine ||
      current.plies !== confirmed.plies ||
      current.search_filter !== confirmed.search_filter);
  const unavailable = !configured
    ? 'engine_unavailable'
    : scoped && gameNo === undefined
      ? 'game'
      : pending
        ? 'configuration'
        : null;
  const selectedGame = scoped ? gameNo : undefined;
  const key = JSON.stringify([
    matchId,
    selectedGame ?? null,
    scoped,
    plies,
    installationKey,
    current,
    confirmed,
    profile?.confirmed?.revision,
    profile?.generation,
    unavailable,
  ]);
  const [result, setResult] = useState<{
    key: string;
    value?: AnalysisTimeEstimateResult;
    failed?: boolean;
  }>();
  const value = result?.key === key ? result.value : undefined;
  const failed = result?.key === key && result.failed;
  useEffect(() => {
    if (unavailable) return;
    const abort = new AbortController();
    void loadAnalysisTimeEstimate(
      matchId,
      plies,
      engineId,
      selectedGame,
      abort.signal,
    )
      .then((value) => {
        if (abort.signal.aborted) return;
        if (
          value.match_id !== matchId ||
          value.game_no !== (selectedGame ?? null) ||
          value.plies !== plies ||
          value.engine_id !== engineId
        )
          throw new Error('Estimate does not match the selected analysis');
        setResult({ key, value });
      })
      .catch(() => {
        if (!abort.signal.aborted) setResult({ key, failed: true });
      });
    return () => abort.abort();
  }, [key, unavailable, matchId, plies, engineId, selectedGame]);
  const duration =
    value?.estimated_ms != null
      ? formatAnalysisDuration(value.estimated_ms)
      : null;
  const reason =
    unavailable ??
    (failed
      ? 'network'
      : value
        ? (value.unavailable_reason ??
          (!duration || !value.benchmark ? 'network' : null))
        : null);
  const available = !!value && !!duration && !!value.benchmark && !reason;
  return (
    <div
      className="analysis-time-estimate"
      role="status"
      aria-live="polite"
      aria-label={text('label')}
    >
      <strong>
        {available
          ? text('estimate', { duration })
          : reason
            ? text('unavailable')
            : text('loading')}
      </strong>
      {available && (
        <>
          <span>
            {text('moves', {
              moves: formatNumber(value.position_count),
              plies: value.plies,
            })}
          </span>
          <small>
            {text('basis', {
              date: new Intl.DateTimeFormat(locale, {
                dateStyle: 'medium',
              }).format(new Date(value.benchmark!.finished_at)),
              engine: value.engine_version ?? value.engine_id,
            })}
          </small>
          <small>{text('caveat')}</small>
        </>
      )}
      {reason && <small>{text(reason)}</small>}
    </div>
  );
}
