import type { Ref } from 'react';
import type { Position } from './api';
import { useI18n } from './i18n';
import { useSourceText } from './analysis-source-messages';
import type { MoveAssessment } from './useMoveAssessments';
import './move-history.css';

const severityLabels = {
  none: 'severity.none',
  doubtful: 'severity.doubtful',
  bad: 'severity.bad',
  very_bad: 'severity.very_bad',
} as const;

export default function MoveHistoryRow({
  position,
  assessment,
  players,
  current,
  onSelect,
  buttonRef,
}: {
  position: Position;
  assessment?: MoveAssessment;
  players: string[];
  current: boolean;
  onSelect: () => void;
  buttonRef?: Ref<HTMLButtonElement>;
}) {
  const { t, formatNumber, formatEquity, formatDecision } = useI18n();
  const source = useSourceText();
  const player = players[position.player];
  const notation = formatDecision(position.notation);
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
    <button
      type="button"
      ref={buttonRef}
      className={`move-history-row${current ? ' current' : ''}${assessment ? ` move-assessment-${assessment.severity}` : ''}`}
      data-position-id={position.id}
      data-assessment-source={assessment?.source}
      aria-current={current ? 'step' : undefined}
      onClick={onSelect}
    >
      <span className="move-no">
        {position.game_no}.{position.ply}
      </span>
      <span className="move-history-details">
        <strong title={notation}>{notation}</strong>
        <span className="move-history-context">
          <i
            aria-hidden="true"
            className={`player-dot ${position.player === 0 ? 'white' : 'black'}`}
          />
          <span className="move-history-player" title={player}>
            {player}
          </span>
          <span aria-hidden="true">·</span>
          <span className="move-dice">{position.dice.join('–')}</span>
        </span>
      </span>
      {assessment ? (
        <span className="move-assessment-marker" title={description}>
          <span className="move-loss-value" aria-hidden="true">
            {loss}
          </span>
          <span className="sr-only">
            {t('matchAnalysis.equityLoss')}: {loss} · {description}
          </span>
        </span>
      ) : (
        <span className="move-assessment-unavailable" title={description}>
          <span aria-hidden="true">—</span>
          <span className="sr-only">{description}</span>
        </span>
      )}
    </button>
  );
}
