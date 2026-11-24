import type { PlaySession } from './play-api';

/** The dialog that stands between the player and the board. */
export type PlayInterruption =
  'resign' | 'hint' | 'cube' | 'feedback' | 'game_over' | 'match_over';

/** A money game is over; its session may continue with another game. */
export const isMoneyGameFinished = (session: PlaySession | null) =>
  session?.match_length === 0 &&
  ['game_over', 'match_over'].includes(session.phase);

/** The decision that waits for the player's answer to its feedback. */
export function playFeedback(session: PlaySession | null) {
  const pending = session?.pending_decision;
  const mode = session?.play_mode ?? 'competition';
  // A roll without a warning continues with the next roll click.
  const routineRoll =
    pending?.action === 'roll' &&
    !pending.warning &&
    (mode === 'training' || (mode === 'coach' && pending.severity === 'none'))
      ? pending
      : null;
  return {
    pending,
    mode,
    competition: mode === 'competition',
    routineRoll,
    awaiting: Boolean(pending?.action && !routineRoll),
  };
}

export function playInterruption({
  session,
  blocked,
  resignOpen,
  hintOpen,
  coaching,
  acknowledged,
}: {
  session: PlaySession | null;
  /** Nothing may interrupt, such as an animation, a review or a pause. */
  blocked: boolean;
  resignOpen: boolean;
  hintOpen: boolean;
  /** Coach feedback has its own dialog. */
  coaching: boolean;
  /** The player has closed the result of this match. */
  acknowledged: boolean;
}): PlayInterruption | null {
  if (blocked) return null;
  const { competition, awaiting } = playFeedback(session);
  if (resignOpen) return 'resign';
  if (hintOpen && !competition) return 'hint';
  if (coaching) return null;
  if (session?.phase === 'double_response') return 'cube';
  if (awaiting) return 'feedback';
  if (session?.phase === 'game_over') return 'game_over';
  if (isMoneyGameFinished(session) && !session?.money_session_ended)
    return 'game_over';
  if (session?.phase === 'match_over' && !acknowledged) return 'match_over';
  return null;
}
