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
import type { DecisionKind } from './match-statistics-api';

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
  kind: DecisionKind;
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
    t(kind === 'cube' ? 'common.cube' : 'common.checker'),
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

export function BoardDecisionComparison({
  played,
  best,
  equities,
  loss,
  source,
  context,
  onReturn,
  onDetails,
}: {
  played: string;
  best: string;
  equities?: { played?: number | null; best?: number | null };
  loss: number | null;
  source: string;
  context?: string;
  onReturn?: () => void;
  onDetails?: () => void;
}) {
  const { t, formatDecision, formatEquity } = useI18n();
  const review = useBoardReviewText();
  const lossText = useCandidateLossText()(loss);
  return (
    <div className="board-decision-comparison">
      {context && (
        <small className="board-decision-provenance">{context}</small>
      )}
      <div className="board-decision-choices">
        <div>
          <span>{t('common.played')}</span>
          <strong>{played ? formatDecision(played) : '—'}</strong>
          {equities?.played != null && (
            <span>
              {t('common.equity')} {formatEquity(equities.played)}
            </span>
          )}
        </div>
        <div>
          <span>{t('matchAnalysis.bestDecision')}</span>
          <strong>{best ? formatDecision(best) : '—'}</strong>
          {equities?.best != null && (
            <span>
              {t('common.equity')} {formatEquity(equities.best)}
            </span>
          )}
        </div>
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
