import { Fragment, useMemo, useState } from 'react';
import type { Position } from '../../../shared/api/api';
import { useAnalysisEntryLabel } from '../AnalysisCatalogSelect';
import { useAnalysisCatalogText } from '../analysis-catalog-messages';
import { useBoardReviewText } from '../../../shared/board/board-review-messages';
import {
  candidateEquityDifference,
  nativeCheckerCandidates,
} from './candidate-comparison';
import {
  candidatePreviewNotation,
  focusCandidateRow,
  navigateCandidatePreview,
  type PreviewCandidate,
} from '../../../shared/review/candidate-preview';
import { useCandidateEquityText, useCandidateText } from './candidate-messages';
import { useI18n } from '../../../shared/i18n/i18n';
import {
  comparisonEngines,
  positionComparisonOptions,
  positionComparisonRows,
  sortPositionComparisonRows,
  xgCheckerCandidates,
  type ComparisonEngine,
  type XgComparisonEngine,
} from './position-comparison';
import { useAnalysisResult, type AnalysisCatalog } from '../useAnalysisCatalog';
import './position-comparison.css';

const names = {
  xg: 'eXtreme Gammon',
  gnubg: 'GNU Backgammon',
  bgsage: 'NeonGammon',
};

export default function PositionComparison({
  position,
  catalog,
  revision,
  active,
  onPreview,
  selectedPreview,
}: {
  position: Position;
  catalog: AnalysisCatalog;
  revision?: string;
  active: boolean;
  onPreview?: (candidate: PreviewCandidate | null) => void;
  selectedPreview?: PreviewCandidate;
}) {
  const text = useCandidateText();
  const review = useBoardReviewText();
  const catalogText = useAnalysisCatalogText();
  const label = useAnalysisEntryLabel();
  const optionLabel = useAnalysisEntryLabel(false);
  const { t, formatDecision, formatNumber, formatError } = useI18n();
  const { unit, equity } = useCandidateEquityText(6);
  const [selection, setSelection] = useState<
    Partial<Record<ComparisonEngine, string>>
  >({});
  const [sortEngine, setSortEngine] = useState<XgComparisonEngine>();
  const entries =
    catalog.selected &&
    !catalog.entries.some((entry) => entry.id === catalog.selected?.id)
      ? [...catalog.entries, catalog.selected]
      : catalog.entries;
  const columns = comparisonEngines.map((engine) => {
    const options = positionComparisonOptions(entries, position, engine);
    const entry =
      options.find((option) => option.id === selection[engine]) ?? options[0];
    return { engine, options, entry };
  });
  const positionId = active ? position.id : undefined;
  const xg = useAnalysisResult(positionId, columns[0].entry?.id, revision);
  const gnubg = useAnalysisResult(positionId, columns[1].entry?.id, revision);
  const sage = useAnalysisResult(positionId, columns[2].entry?.id, revision);
  const results = [xg, gnubg, sage];
  const candidates = useMemo(
    () => [
      xgCheckerCandidates(xg.result, position.player),
      nativeCheckerCandidates(gnubg.result),
      nativeCheckerCandidates(sage.result),
    ],
    [xg.result, gnubg.result, sage.result, position.player],
  );
  const rows = useMemo(
    () => positionComparisonRows(position, candidates),
    [position, candidates],
  );
  const orderedRows = useMemo(
    () => (sortEngine ? sortPositionComparisonRows(rows, sortEngine) : rows),
    [rows, sortEngine],
  );
  const units = candidates.map((list) => [
    ...new Set(list.map((row) => row.equity_kind)),
  ]);
  const hasXgComparison =
    candidates[0].length > 0 &&
    (candidates[1].length > 0 || candidates[2].length > 0);
  const loading = catalog.loading || results.some((result) => result.loading);
  const unknown = '—';
  const previewFor = (
    row: (typeof rows)[number],
    index = row.candidates.findIndex(Boolean),
  ): PreviewCandidate | null => {
    const candidate = row.candidates[index];
    if (!candidate) return null;
    const { engine, entry } = columns[index];
    return {
      notation: candidatePreviewNotation(candidate),
      rank:
        engine === 'xg' ? (row.xgEquityRank ?? candidate.rank) : candidate.rank,
      source: entry?.engine ?? names[engine],
    };
  };
  const isSelected = (move: PreviewCandidate | null) =>
    !!move &&
    selectedPreview?.notation === move.notation &&
    selectedPreview.source === move.source;
  return (
    <div className="position-comparison-scroll">
      <table
        className="candidates position-comparison-table"
        aria-label={text('compareMoves')}
      >
        <colgroup>
          <col className="position-comparison-move" />
        </colgroup>
        {comparisonEngines.map((engine) => (
          <colgroup key={engine}>
            <col className="position-comparison-rank" />
            <col className="position-comparison-equity" />
            <col className="position-comparison-depth" />
          </colgroup>
        ))}
        <thead>
          <tr>
            <th scope="col" rowSpan={2}>
              {t('common.move')}
            </th>
            {columns.map(({ engine, options, entry }, index) => (
              <th
                scope="colgroup"
                colSpan={3}
                className="position-comparison-group"
                key={engine}
              >
                <strong>{names[engine]}</strong>
                {options.length > 1 ? (
                  <select
                    aria-label={text('comparisonSource', {
                      engine: names[engine],
                    })}
                    value={entry?.id ?? ''}
                    title={entry && label(entry)}
                    onChange={(event) => {
                      onPreview?.(null);
                      setSelection({
                        ...selection,
                        [engine]: event.target.value,
                      });
                    }}
                  >
                    {options.map((option) => (
                      <option key={option.id} value={option.id}>
                        {optionLabel(option)}
                      </option>
                    ))}
                  </select>
                ) : (
                  entry &&
                  engine !== 'xg' && (
                    <small title={label(entry)}>{entry.depth}</small>
                  )
                )}
                {units[index].length === 1 && (
                  <small>{unit(units[index][0])}</small>
                )}
                {results[index].loading && (
                  <small role="status">{catalogText('loading')}</small>
                )}
                {!!results[index].error && (
                  <small role="alert">
                    {formatError(results[index].error)}
                  </small>
                )}
                {!loading &&
                  !results[index].error &&
                  candidates[index].length === 0 && (
                    <small>{text('noCandidates')}</small>
                  )}
              </th>
            ))}
          </tr>
          <tr>
            {comparisonEngines.map((engine, index) => {
              const canSort =
                engine !== 'xg' &&
                !loading &&
                rows.some(
                  (row) =>
                    candidateEquityDifference(
                      row.candidates[index],
                      row.candidates[0],
                    ) !== null,
                );
              const sorted = canSort && sortEngine === engine;
              const sortLabel = text('sortByXgDifference', {
                engine: names[engine],
              });
              return (
                <Fragment key={engine}>
                  <th scope="col" className="position-comparison-group">
                    {t('analysis.rank')}
                  </th>
                  <th scope="col" aria-sort={sorted ? 'other' : undefined}>
                    {t('common.equity')}
                    {engine !== 'xg' && (
                      <button
                        type="button"
                        className="position-comparison-sort"
                        aria-label={sortLabel}
                        aria-pressed={sorted}
                        title={sortLabel}
                        disabled={!canSort}
                        onClick={() =>
                          setSortEngine((current) =>
                            current === engine ? undefined : engine,
                          )
                        }
                      >
                        <span aria-hidden="true">|Δ|{sorted && '↓'}</span>
                      </button>
                    )}
                  </th>
                  <th scope="col">{t('analysis.depth')}</th>
                </Fragment>
              );
            })}
          </tr>
        </thead>
        <tbody
          onKeyDown={
            onPreview &&
            ((event) =>
              navigateCandidatePreview(event, (index, column) =>
                onPreview(
                  previewFor(
                    orderedRows[index],
                    column === undefined ? undefined : Number(column),
                  ),
                ),
              ))
          }
        >
          {!loading &&
            orderedRows.map((row) => {
              const moves = onPreview
                ? row.candidates.map((_, index) => previewFor(row, index))
                : [];
              const move = moves.find(Boolean);
              const notation = formatDecision(row.notation, position);
              const selectedColumn = moves.findIndex(isSelected);
              return (
                <tr
                  key={row.key}
                  data-candidate-played={row.played}
                  data-preview-selected={selectedColumn !== -1}
                  className={move ? 'candidate-preview-row' : undefined}
                  onClick={
                    move && onPreview
                      ? (event) => {
                          const column =
                            event.target instanceof Element
                              ? event.target.closest<HTMLElement>(
                                  'td[data-preview-column]',
                                )?.dataset.previewColumn
                              : undefined;
                          const selected =
                            column === undefined ? move : moves[Number(column)];
                          if (!selected) return;
                          focusCandidateRow(event.currentTarget, column);
                          onPreview(selected);
                        }
                      : undefined
                  }
                >
                  <th scope="row">
                    {move ? (
                      <button
                        type="button"
                        className="candidate-preview-button"
                        aria-label={review('previewMove', {
                          rank: move.rank,
                          move: notation,
                        })}
                        aria-pressed={isSelected(move)}
                      >
                        {notation}
                      </button>
                    ) : (
                      notation
                    )}
                    {row.played && <small>{text('played')}</small>}
                  </th>
                  {row.candidates.map((candidate, index) => {
                    if (!candidate)
                      return (
                        <td
                          key={comparisonEngines[index]}
                          colSpan={3}
                          className="position-comparison-group position-comparison-unavailable"
                          data-preview-column={index}
                        >
                          {unknown}
                        </td>
                      );
                    const local = index > 0;
                    const xgRank = row.xgEquityRank;
                    const rank = local ? candidate.rank : xgRank;
                    const rankChanged =
                      local &&
                      xgRank !== undefined &&
                      candidate.rank !== xgRank;
                    const delta = local
                      ? candidateEquityDifference(candidate, row.candidates[0])
                      : null;
                    const rankReference = rankChanged
                      ? text('xgRank', { rank: formatNumber(xgRank) })
                      : undefined;
                    const deltaText =
                      delta === null
                        ? unknown
                        : candidate.equity_kind === 'mwc'
                          ? text('percentagePoints', {
                              value: formatNumber(delta * 100, {
                                signDisplay: 'always',
                                minimumFractionDigits: 6,
                                maximumFractionDigits: 6,
                              }),
                            })
                          : equity(delta, candidate.equity_kind);
                    const cell = {
                      'data-preview-column': index,
                      'data-engine-preview-selected':
                        selectedColumn === index || undefined,
                    };
                    const rankLabel =
                      rank === undefined ? unknown : formatNumber(rank);
                    return (
                      <Fragment key={comparisonEngines[index]}>
                        <td
                          {...cell}
                          className="position-comparison-group position-comparison-rank-value"
                          data-xg-rank-difference={rankChanged || undefined}
                          title={rankReference}
                        >
                          {moves[index] ? (
                            <button
                              type="button"
                              className="candidate-preview-button"
                              data-preview-column={index}
                              aria-label={review('previewEngineMove', {
                                move: notation,
                                engine: names[comparisonEngines[index]],
                              })}
                              aria-pressed={selectedColumn === index}
                            >
                              {rankLabel}
                            </button>
                          ) : (
                            rankLabel
                          )}
                          {rankChanged && (
                            <span
                              className="position-comparison-rank-arrow"
                              aria-hidden="true"
                            >
                              {candidate.rank < xgRank ? '↑' : '↓'}
                            </span>
                          )}
                        </td>
                        <td {...cell} className="position-comparison-number">
                          {equity(candidate.equity, candidate.equity_kind)}
                          {units[index].length > 1 && (
                            <small>{unit(candidate.equity_kind)}</small>
                          )}
                          {local && hasXgComparison && (
                            <small className="position-comparison-delta">
                              Δ {deltaText}
                            </small>
                          )}
                        </td>
                        <td {...cell}>{candidate.depth ?? unknown}</td>
                      </Fragment>
                    );
                  })}
                </tr>
              );
            })}
        </tbody>
      </table>
      {!loading && hasXgComparison && (
        <p className="fineprint">{text('xgComparison')}</p>
      )}
    </div>
  );
}
