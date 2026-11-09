import type { Position } from './api';
import type { BoardView, MovePreview } from './move-preview';

/** One source for both checker rendering and board-only contextual information. */
export function displayedBoardPosition(
  position: Position,
  preview?: MovePreview,
  view: BoardView = 'before',
): Position {
  return view === 'after' && preview?.ok
    ? { ...position, board: preview.board, dice: [] }
    : position;
}
