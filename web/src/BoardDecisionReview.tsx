import type { ReactNode } from 'react';
import ActionButton from './ActionButton';
import EngineLabel from './EngineLabel';
import { useBoardReviewText } from './board-review-messages';
import { useWorkspaceText } from './workspace-messages';
import {
  severityLabels,
  useStatisticsText,
} from './review-statistics-messages';
import { useCandidateLossText } from './candidate-messages';
import { useI18n } from './i18n';
import type { Position } from './api';
import type { MoveAssessment } from './useMoveAssessments';
import type { ReviewDecision } from './review-decisions-api';
import type { PreviewCandidate, PreviewComparison } from './candidate-preview';

export function BoardErrorNavigation({
  game,
  player,
  kind,
  severity,
  errorsOnly,
  total,
  ordinal,
  busy,
  onNavigate,
  children,
}: {
  game?: number;
  player?: string;
  kind: string;
  severity?: string;
  errorsOnly: boolean;
  total: number;
  ordinal?: number;
  busy: boolean;
  onNavigate: (direction: 'previous' | 'next') => void;
  children?: ReactNode;
}) {
  const { t, formatNumber } = useI18n();
  const review = useBoardReviewText();
  const workspace = useWorkspaceText();
  const statistics = useStatisticsText();
  const context = [
    game === undefined ? statistics('allGames') : statistics('game', { game }),
    player ?? t('matchAnalysis.bothPlayers'),
    kind === 'checker'
      ? t('common.checker')
      : kind === 'cube'
        ? t('common.cube')
        : t('matchAnalysis.bothKinds'),
    severity && severityLabels[severity]
      ? t(severityLabels[severity])
      : undefined,
  ]
    .filter(Boolean)
    .join(' · ');
  return (
    <div
      className="board-error-review"
      role="group"
      aria-label={review('errorNavigation')}
    >
      <div className="board-error-controls">
        <ActionButton
          type="button"
          icon="back"
          label={review(errorsOnly ? 'previousError' : 'previousDecision')}
          disabled={total === 0}
          aria-disabled={busy || undefined}
          onClick={() => onNavigate('previous')}
        />
        <ActionButton
          type="button"
          icon="next"
          label={errorsOnly ? workspace('nextError') : review('nextDecision')}
          disabled={total === 0}
          aria-disabled={busy || undefined}
          onClick={() => onNavigate('next')}
        />
      </div>
      <div className="board-error-context">
        {total > 0 && (
          <small className="board-error-count" role="status">
            {ordinal === undefined
              ? review(errorsOnly ? 'errorTotal' : 'decisionTotal', {
                  total: formatNumber(total),
                })
              : review(errorsOnly ? 'errorOrdinal' : 'decisionOrdinal', {
                  index: formatNumber(ordinal),
                  total: formatNumber(total),
                })}
          </small>
        )}
        <small className="board-decision-provenance">{context}</small>
      </div>
      {children}
    </div>
  );
}

export function BoardCheckerComparison({
  position,
  assessment,
  candidateComparison,
  reviewRow,
  selectedPreview,
  onPreview,
}: {
  position: Position;
  assessment?: MoveAssessment;
  candidateComparison?: PreviewComparison;
  reviewRow?: ReviewDecision;
  selectedPreview?: PreviewCandidate;
  onPreview: (candidate: PreviewCandidate | null) => void;
}) {
  const saved =
    reviewRow?.evaluation_status === 'evaluated'
      ? {
          best_move: reviewRow.best,
          engine: reviewRow.engine,
          depth: reviewRow.depth,
          equity_loss: reviewRow.equity_loss,
        }
      : assessment;
  const source = [saved?.engine, saved?.depth].filter(Boolean).join(' · ');
  const useSaved =
    candidateComparison?.useSavedGame || candidateComparison === undefined;
  const fallback =
    !candidateComparison?.best && useSaved && saved?.best_move
      ? { notation: saved.best_move, rank: 1, source }
      : undefined;
  const best = candidateComparison?.best ?? fallback;
  if (!best) return null;
  return (
    <BoardDecisionComparison
      played={position.notation}
      best={best.notation}
      loss={
        fallback
          ? (saved?.equity_loss ?? null)
          : (candidateComparison?.loss ?? null)
      }
      lossUnit={fallback ? undefined : candidateComparison?.lossUnit}
      source={best.source}
      selected={
        selectedPreview
          ? selectedPreview.source === best.source &&
            selectedPreview.rank === best.rank &&
            selectedPreview.notation === best.notation
            ? 'best'
            : undefined
          : 'played'
      }
      onPlayed={() => onPreview(null)}
      onBest={() => onPreview(best)}
    />
  );
}

export function BoardDecisionComparison({
  played,
  best,
  loss,
  lossUnit,
  source,
  selected,
  onPlayed,
  onBest,
  context,
  onReturn,
  onDetails,
}: {
  played: string;
  best: string;
  loss: number | null;
  lossUnit?: 'mwc';
  source: string;
  selected?: 'played' | 'best';
  onPlayed?: () => void;
  onBest?: () => void;
  context?: string;
  onReturn?: () => void;
  onDetails?: () => void;
}) {
  const { t, formatDecision } = useI18n();
  const review = useBoardReviewText();
  const lossText = useCandidateLossText()(loss, lossUnit);
  return (
    <div className="board-decision-comparison">
      {context && (
        <small className="board-decision-provenance">{context}</small>
      )}
      <div className="board-decision-choices">
        {(
          [
            [
              'played',
              onPlayed ? review('played') : t('common.played'),
              played,
              onPlayed,
            ],
            [
              'best',
              onBest ? review('bestMove') : t('matchAnalysis.bestDecision'),
              best,
              onBest,
            ],
          ] as const
        ).map(([kind, label, value, action]) => {
          const content = (
            <>
              <span>{label}</span>
              <strong>{value ? formatDecision(value) : '—'}</strong>
            </>
          );
          return action ? (
            <button
              key={kind}
              type="button"
              aria-pressed={selected === kind}
              onClick={action}
            >
              {content}
            </button>
          ) : (
            <div key={kind}>{content}</div>
          );
        })}
      </div>
      <small className="board-decision-provenance">
        <strong>
          {t('matchAnalysis.loss')} {lossText}
        </strong>
        {source && (
          <>
            {' · '}
            <EngineLabel details={source} />
          </>
        )}
      </small>
      {(onReturn || onDetails) && (
        <div className="board-decision-actions">
          {onReturn && (
            <ActionButton
              type="button"
              icon="back"
              label={review('returnToMove')}
              onClick={onReturn}
            />
          )}
          {onDetails && (
            <ActionButton
              type="button"
              icon="open"
              label={review('cubeDetails')}
              onClick={onDetails}
            />
          )}
        </div>
      )}
    </div>
  );
}
