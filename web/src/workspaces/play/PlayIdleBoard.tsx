import type { RefObject } from 'react';
import Board from '../../shared/board/Board';
import type { BearoffDirection } from '../../shared/board/play-state';
import type { Theme } from '../../shared/board/themes';
import type { PlaySession } from './play-api';
import { usePlayText } from './play-messages';

/** Starting position on the empty table behind the setup dialog. */
const idlePosition: PlaySession['position'] = {
  id: '',
  match_uuid: '',
  ordinal: 0,
  game_no: 1,
  ply: 0,
  player: 0,
  dice: [],
  board: [
    0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0,
    2, 0,
  ],
  position_id: '4HPwATDgc/ABMA',
  // Centered cube at 1, no score: the empty table carries no match context.
  gnubg_match_id: 'MAAAAAAAAAAA',
  notation: '',
};
/** The empty table that stays visible behind the setup dialog. */
export default function PlayIdleBoard({
  anchor,
  human,
  direction,
  inert,
  theme,
}: {
  anchor: RefObject<HTMLDivElement | null>;
  human: number;
  direction: BearoffDirection;
  inert: boolean;
  theme: Theme;
}) {
  const t = usePlayText();
  return (
    <div
      className="play-board play-idle-board"
      inert={inert}
      ref={anchor}
      data-human-player={human}
    >
      <Board
        playOrientation={{ human, direction }}
        position={idlePosition}
        white={t(human === 0 ? 'you' : 'bot')}
        black={t(human === 1 ? 'you' : 'bot')}
        theme={theme}
      />
    </div>
  );
}
