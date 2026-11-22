import { useEffect, useMemo, useState } from 'react';
import { request } from '../../../shared/api/api';
import { useI18n } from '../../../shared/i18n/i18n';
import { nativePerformanceRating } from './match-statistics-api';
import { useStatisticsText } from '../../../shared/review/review-statistics-messages';
import {
  analysisStatisticsPath,
  type AnalysisCatalogEntry,
  type MatchAnalysisStatistics,
} from '../analysis-catalog-api';
import type { AnalysisCatalog } from '../useAnalysisCatalog';
import { useAnalysisCatalogText } from '../analysis-catalog-messages';
import { useAnalysisEntryLabel } from '../AnalysisCatalogSelect';
import { prDigits } from '../../../shared/format/pr-format';

export function comparisonEntries(
  entries: AnalysisCatalogEntry[],
  selection: string[] = [],
) {
  return [0, 1].map(
    (column) =>
      entries.find((entry) => entry.id === selection[column]) ??
      entries[column],
  );
}

export function MatchAnalysisComparisonResults({
  entries,
  statistics,
  names,
  gameNo,
  selection = [],
  onSelection,
}: {
  entries: AnalysisCatalogEntry[];
  statistics: Record<string, MatchAnalysisStatistics>;
  names: string[];
  gameNo?: number;
  selection?: string[];
  onSelection?: (column: number, id: string) => void;
}) {
  const { formatNumber } = useI18n();
  const text = useStatisticsText();
  const catalogText = useAnalysisCatalogText();
  const label = useAnalysisEntryLabel();
  const columns = comparisonEntries(entries, selection).map((entry) => {
    const saved = entry && statistics[entry.id];
    const result = saved?.analysis_id === entry?.id ? saved : undefined;
    const summary =
      gameNo == null
        ? result?.summary
        : result?.games.find((game) => game.game_no === gameNo)?.summary;
    return { entry, summary };
  });
  const decimal = (value: number) =>
    formatNumber(value, { minimumFractionDigits: 3, maximumFractionDigits: 3 });
  const unavailable = <span title={text('unavailable')}>—</span>;
  return (
    <div className="statistics-comparison-scroll">
      <table className="statistics-games-table statistics-comparison-table">
        <thead>
          <tr>
            <th scope="col">{text('compareMetric')}</th>
            {columns.map(({ entry }, column) => (
              <th key={column} scope="col">
                <select
                  aria-label={catalogText('comparison', { number: column + 1 })}
                  value={entry?.id ?? ''}
                  onChange={(event) =>
                    onSelection?.(column, event.target.value)
                  }
                >
                  {!entry && <option value="">{catalogText('none')}</option>}
                  {entries.map((option) => (
                    <option value={option.id} key={option.id}>
                      {label(option)}
                    </option>
                  ))}
                </select>
                {entry && (
                  <small>
                    {entry.engine}
                    {entry.depth ? ` · ${entry.depth}` : ''}
                  </small>
                )}
              </th>
            ))}
          </tr>
        </thead>
        {names.map((name, player) => {
          const stats = columns.map(({ summary }) =>
            summary?.players.find((entry) => entry.player === player),
          );
          const rows = [
            [
              text('compareCheckerCount'),
              stats.map(
                (entry) => entry && formatNumber(entry.checker_decisions),
              ),
            ],
            [
              text('compareCubeCount'),
              stats.map((entry) => entry && formatNumber(entry.cube_decisions)),
            ],
            [
              text('compareCheckerLoss'),
              stats.map((entry) => entry && decimal(entry.checker_equity_loss)),
            ],
            [
              text('compareCubeLoss'),
              stats.map((entry) => entry && decimal(entry.cube_equity_loss)),
            ],
            [
              text('compareUnavailable'),
              stats.map((entry) => entry && formatNumber(entry.unavailable)),
            ],
            [
              'GNUbg PR',
              columns.map(({ entry, summary }) => {
                const rating =
                  entry?.source === 'native' && summary
                    ? nativePerformanceRating(summary, player)
                    : undefined;
                return rating === undefined
                  ? undefined
                  : formatNumber(rating, prDigits);
              }),
            ],
          ] as const;
          return (
            <tbody key={player}>
              <tr className="statistics-comparison-player">
                <th scope="rowgroup" colSpan={3}>
                  {name}
                </th>
              </tr>
              {rows.map(([label, values]) => (
                <tr key={label}>
                  <th scope="row">{label}</th>
                  {values.map((value, column) => (
                    <td key={column}>{value ?? unavailable}</td>
                  ))}
                </tr>
              ))}
            </tbody>
          );
        })}
      </table>
    </div>
  );
}

export default function MatchAnalysisComparison({
  matchId,
  gameNo,
  names,
  visible,
  catalog,
}: {
  matchId: string;
  gameNo?: number;
  names: string[];
  visible: boolean;
  catalog: AnalysisCatalog;
}) {
  const { formatError } = useI18n();
  const text = useAnalysisCatalogText();
  const [open, setOpen] = useState(false);
  const [selection, setSelection] = useState<string[]>([]);
  const entries = useMemo(
    () =>
      catalog.selected &&
      !catalog.entries.some((entry) => entry.id === catalog.selected!.id)
        ? [catalog.selected, ...catalog.entries]
        : catalog.entries,
    [catalog.entries, catalog.selected],
  );
  const chosen = comparisonEntries(entries, selection);
  const identity = JSON.stringify([
    matchId,
    catalog.sourceRevision,
    ...chosen.map((entry) => [
      entry?.id,
      entry?.status,
      entry?.job?.completed_games,
    ]),
  ]);
  const [state, setState] = useState<{
    identity: string;
    statistics: Record<string, MatchAnalysisStatistics>;
    errors: unknown[];
  }>();
  useEffect(() => {
    if (!open || !visible) return;
    const abort = new AbortController();
    const ids = [
      ...new Set(chosen.flatMap((entry) => (entry ? [entry.id] : []))),
    ];
    void Promise.allSettled(
      ids.map((id) =>
        request<MatchAnalysisStatistics>(analysisStatisticsPath(matchId, id), {
          signal: abort.signal,
        }),
      ),
    ).then((results) => {
      if (abort.signal.aborted) return;
      setState({
        identity,
        statistics: Object.fromEntries(
          results.flatMap((result, index) =>
            result.status === 'fulfilled' &&
            result.value.analysis_id === ids[index]
              ? [[ids[index], result.value]]
              : [],
          ),
        ),
        errors: results.flatMap((result) =>
          result.status === 'rejected' ? [result.reason] : [],
        ),
      });
    });
    return () => abort.abort();
  }, [identity, open, visible, matchId]);
  const current = state?.identity === identity ? state : undefined;
  return (
    <details
      className="statistics-comparison"
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary>{text('compare')}</summary>
      {open && !current && <p role="status">{text('loading')}</p>}
      {current?.errors.map((error, index) => (
        <p role="alert" key={index}>
          {formatError(error)}
        </p>
      ))}
      {open && (
        <MatchAnalysisComparisonResults
          entries={entries}
          statistics={current?.statistics ?? {}}
          names={names}
          gameNo={gameNo}
          selection={selection}
          onSelection={(column, id) =>
            setSelection(
              chosen.map((entry, index) =>
                index === column ? id : (entry?.id ?? ''),
              ),
            )
          }
        />
      )}
      {catalog.nextCursor && (
        <button
          type="button"
          onClick={() => void catalog.loadMore()}
          disabled={catalog.loadingMore}
        >
          {text('more')}
        </button>
      )}
    </details>
  );
}
