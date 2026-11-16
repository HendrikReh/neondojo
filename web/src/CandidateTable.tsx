import { useMemo } from 'react';
import EngineLabel from './EngineLabel';
import { useI18n } from './i18n';
import { useSourceText } from './analysis-source-messages';
import { useBoardReviewText } from './board-review-messages';
import {
  useCandidateEquityText,
  useCandidateLossText,
  useCandidateText,
} from './candidate-messages';
import {
  candidatePreviewNotation,
  focusCandidateRow,
  navigateCandidatePreview,
  type PreviewCandidate,
} from './candidate-preview';
import {
  candidateLoss,
  candidateOutcomes,
  isPlayedCandidate,
  type CandidatePosition,
  type ComparisonCandidate,
} from './candidate-comparison';
import MoveExplanation from './MoveExplanation';
import { useMoveExplanationText } from './move-explanation-messages';
import './candidate-table.css';

const outcomeColumns = ['total', 'single', 'gammon', 'backgammon'] as const;
const outcomeSides = ['win', 'loss'] as const;

export default function CandidateTable({
  candidates,
  source,
  depth,
  showCandidateDepth = false,
  played,
  position,
  playerName,
  onPreview,
  selectedPreview,
}: {
  candidates: ComparisonCandidate[];
  source: string;
  depth?: string | null;
  showCandidateDepth?: boolean;
  played?: string;
  position?: CandidatePosition;
  playerName?: string;
  onPreview?: (candidate: PreviewCandidate | null) => void;
  selectedPreview?: PreviewCandidate;
}) {
  const { t, formatNumber, formatPercent, formatDecision } = useI18n();
  const l = useCandidateText();
  const lossText = useCandidateLossText();
  const sourceText = useSourceText();
  const review = useBoardReviewText();
  const glossary = useMoveExplanationText();
  const best = candidates.find((candidate) => candidate.rank === 1);
  const units = [
    ...new Set(candidates.map((candidate) => candidate.equity_kind)),
  ];
  const { unit, equity } = useCandidateEquityText();
  const playedRows = useMemo(
    () =>
      candidates.map((candidate) =>
        isPlayedCandidate(
          candidatePreviewNotation(candidate),
          played,
          position,
        ),
      ),
    [candidates, played, position],
  );
  const playedCandidate = candidates[playedRows.indexOf(true)];
  const preview = (candidate: ComparisonCandidate) =>
    onPreview?.({
      notation: candidatePreviewNotation(candidate),
      rank: candidate.rank,
      source,
    });
  return (
    <div className="candidate-comparison">
      <p className="candidate-metadata">
        <strong>
          <EngineLabel details={source} />
        </strong>
        {units.length > 0 && (
          <span>{units.length === 1 ? unit(units[0]) : l('mixedUnits')}</span>
        )}
        <span>{l('perspective', { player: playerName || l('onRoll') })}</span>
      </p>
      {candidates.length === 0 ? (
        <p className="fineprint" role="status">
          {l('noCandidates')}
        </p>
      ) : (
        <div className="source-candidates">
          <table className="candidates candidate-comparison-table">
            <colgroup span={4} />
            <colgroup span={outcomeColumns.length} />
            <thead>
              <tr>
                <th scope="col" rowSpan={2}>
                  {t('analysis.rank')}
                </th>
                <th scope="col" rowSpan={2} className="candidate-move-heading">
                  {t('common.move')}
                </th>
                <th
                  scope="col"
                  rowSpan={2}
                  className="candidate-number"
                  title={glossary('equity')}
                >
                  {t('common.equity')}
                </th>
                <th
                  scope="col"
                  rowSpan={2}
                  className="candidate-number"
                  title={l('lossHelp')}
                >
                  {l('loss')}
                </th>
                <th
                  scope="colgroup"
                  colSpan={outcomeColumns.length}
                  className="candidate-outcome-heading"
                  title={l('outcomeHelp')}
                >
                  {l('outcomeHeading')}
                </th>
              </tr>
              <tr>
                {outcomeColumns.map((column) => (
                  <th
                    scope="col"
                    key={column}
                    className="candidate-number"
                    data-outcome={column}
                    aria-label={l(`outcome_${column}`)}
                  >
                    {column === 'backgammon' ? (
                      <>
                        <span className="candidate-backgammon-full">
                          {l('outcome_backgammon')}
                        </span>
                        <abbr
                          className="candidate-backgammon-short"
                          title={l('outcome_backgammon')}
                        >
                          BG
                        </abbr>
                      </>
                    ) : (
                      l(`outcome_${column}`)
                    )}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody
              onKeyDown={
                onPreview
                  ? (event) =>
                      navigateCandidatePreview(event, (index) =>
                        preview(candidates[index]),
                      )
                  : undefined
              }
            >
              {candidates.map((candidate, index) => {
                const notation = formatDecision(candidate.notation, position);
                const selected =
                  !!onPreview &&
                  (selectedPreview
                    ? selectedPreview.notation ===
                        candidatePreviewNotation(candidate) &&
                      selectedPreview.source === source
                    : playedRows[index]);
                const loss = candidateLoss(candidate, best);
                const outcomes = candidateOutcomes(candidate.probabilities);
                return (
                  <tr
                    key={candidate.rank}
                    className={onPreview ? 'candidate-preview-row' : undefined}
                    data-candidate-best={candidate.rank === 1}
                    data-candidate-played={playedRows[index]}
                    data-preview-selected={selected}
                    onClick={
                      onPreview
                        ? (event) => {
                            focusCandidateRow(event.currentTarget);
                            preview(candidate);
                          }
                        : undefined
                    }
                  >
                    <td>{formatNumber(candidate.rank)}</td>
                    <td>
                      {onPreview ? (
                        <button
                          type="button"
                          className="candidate-preview-button"
                          aria-label={review('previewMove', {
                            rank: candidate.rank,
                            move: notation,
                          })}
                          aria-pressed={selected}
                        >
                          {notation}
                        </button>
                      ) : (
                        notation
                      )}
                      <span className="candidate-markers">
                        {candidate.rank === 1 && (
                          <span className="candidate-marker candidate-marker-best">
                            {l('best')}
                          </span>
                        )}
                        {playedRows[index] && (
                          <span className="candidate-marker candidate-marker-played">
                            {l('played')}
                          </span>
                        )}
                        {selected && selectedPreview && (
                          <span className="candidate-marker candidate-marker-preview">
                            {l('preview')}
                          </span>
                        )}
                      </span>
                      {(showCandidateDepth ||
                        (candidate.depth ?? null) !== (depth ?? null)) && (
                        <small className="candidate-depth">
                          {candidate.depth ?? sourceText('unknownDepth')}
                        </small>
                      )}
                    </td>
                    <td className="candidate-number">
                      {equity(candidate.equity, candidate.equity_kind)}
                      {units.length > 1 && (
                        <small>{unit(candidate.equity_kind)}</small>
                      )}
                    </td>
                    <td className="candidate-number" title={l('lossHelp')}>
                      {lossText(loss, candidate.equity_kind)}
                    </td>
                    {outcomeColumns.map((column) => (
                      <td
                        key={column}
                        className="candidate-number"
                        data-outcome={column}
                      >
                        {outcomeSides.map((side) => {
                          const value = outcomes[side][column];
                          const label = l(
                            side === 'win' ? 'gameWin' : 'gameLoss',
                          );
                          return (
                            <span
                              key={side}
                              className={`candidate-probability-line ${side}`}
                              title={label}
                            >
                              <span className="sr-only">{label} </span>
                              <span data-probability={side}>
                                {value === null ? '—' : formatPercent(value, 1)}
                              </span>
                            </span>
                          );
                        })}
                      </td>
                    ))}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {position && best && playedCandidate && playedCandidate !== best && (
        <MoveExplanation
          position={position}
          best={candidatePreviewNotation(best)}
          played={candidatePreviewNotation(playedCandidate)}
          bestChances={best.probabilities}
          playedChances={playedCandidate.probabilities}
        />
      )}
    </div>
  );
}
