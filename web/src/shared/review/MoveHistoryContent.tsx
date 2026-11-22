import type { ReactNode } from 'react';
import type { Position } from '../api/api';
import { useI18n } from '../i18n/i18n';
import { useSourceText } from '../../workspaces/analysis/candidates/analysis-source-messages';
import { clippedTooltipText } from '../format/overflow-title';
import type { MoveAssessment } from './useMoveAssessments';
import { severityLabels } from './review-statistics-messages';
import './move-history.css';

export function moveHistoryTooltip(anchor: Element, firstAttempt?: string) {
  return [
    clippedTooltipText(anchor),
    firstAttempt,
    anchor
      .querySelector('[data-move-assessment]')
      ?.getAttribute('data-move-assessment'),
  ]
    .filter(Boolean)
    .join(' · ');
}

export default function MoveHistoryContent({
  position,
  assessment,
  players,
  moveNumber,
}: {
  position: Pick<Position, 'player' | 'dice' | 'notation'> &
    Partial<Pick<Position, 'board'>>;
  assessment?: Pick<
    MoveAssessment,
    'equity_loss' | 'severity' | 'source' | 'engine' | 'depth'
  >;
  players: string[];
  moveNumber: ReactNode;
}) {
  const { t, formatNumber, formatEquity, formatDecision } = useI18n();
  const source = useSourceText();
  const player = players[position.player];
  const { board, dice } = position;
  const notation = formatDecision(
    position.notation,
    board ? { board, dice, player: position.player } : undefined,
  );
  const loss = assessment
    ? formatNumber(assessment.equity_loss, {
        minimumFractionDigits: 3,
        maximumFractionDigits: 3,
      })
    : undefined;
  const description = assessment
    ? `${t(severityLabels[assessment.severity])} · Δ ${formatEquity(assessment.equity_loss)} · ${source(assessment.source === 'imported' ? 'imported' : 'engine')} · ${assessment.engine}${assessment.depth ? ` · ${assessment.depth}` : ''}`
    : t('matchAnalysis.unavailable');
  return (
    <>
      <span className="move-no">{moveNumber}</span>
      <span className="move-history-details">
        <strong data-tooltip-overflow>{notation}</strong>
        <span className="move-history-context">
          <i
            aria-hidden="true"
            className={`player-dot ${position.player === 0 ? 'white' : 'black'}`}
          />
          <span className="move-history-player" data-tooltip-overflow>
            {player}
          </span>
          {position.dice.length > 0 && (
            <>
              <span aria-hidden="true">·</span>
              <span className="move-dice">{position.dice.join('–')}</span>
            </>
          )}
        </span>
      </span>
      {assessment ? (
        <span
          className="move-assessment-marker"
          data-move-assessment={description}
        >
          <span className="move-loss-value" aria-hidden="true">
            {loss}
          </span>
          <span className="sr-only">
            {t('matchAnalysis.equityLoss')}: {loss} · {description}
          </span>
        </span>
      ) : (
        <span
          className="move-assessment-unavailable"
          data-move-assessment={description}
        >
          <span aria-hidden="true">—</span>
          <span className="sr-only">{description}</span>
        </span>
      )}
    </>
  );
}
