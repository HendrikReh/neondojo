import type { Ref } from 'react';
import type { Position } from './api';
import type { MoveAssessment } from './useMoveAssessments';
import MoveHistoryContent, { moveHistoryTooltip } from './MoveHistoryContent';
import Tooltip from './Tooltip';

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
  return (
    <Tooltip
      as="button"
      text={moveHistoryTooltip}
      type="button"
      ref={buttonRef}
      className={`move-history-row${current ? ' current' : ''}${assessment ? ` move-assessment-${assessment.severity}` : ''}`}
      data-position-id={position.id}
      data-assessment-source={assessment?.source}
      aria-current={current ? 'step' : undefined}
      onClick={onSelect}
    >
      <MoveHistoryContent
        position={position}
        assessment={assessment}
        players={players}
        moveNumber={`${position.game_no}.${position.ply}`}
      />
    </Tooltip>
  );
}
