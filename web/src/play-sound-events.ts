import type { PlayAction, PlaySession } from './play-api';
import { applySteps, type CheckerStep } from './play-state';
export type PlaySound = 'move' | 'roll' | 'hit' | 'bear_off' | 'cube';
export type SoundState = Pick<
  PlaySession,
  'id' | 'version' | 'human_player' | 'history' | 'phase'
> & { position: { board: number[] } };

export function isNewCubeOffer(before: SoundState, after: SoundState) {
  return (
    before.id === after.id &&
    after.version > before.version &&
    before.phase !== 'double_response' &&
    after.phase === 'double_response' &&
    after.history
      .slice(before.history.length)
      .some(
        (row) =>
          row.player !== before.human_player &&
          row.notation.toLowerCase() === 'double',
      )
  );
}

export function checkerSound(
  board: number[],
  step: CheckerStep,
  player: number,
): PlaySound {
  if (step.to === -1 || step.to === 26) return 'bear_off';
  return board[step.to] === (player === 0 ? -1 : 1) ? 'hit' : 'move';
}

/** Native history points are relative to the mover, unlike the UI's board. */
export function nativeSteps(notation: string, player: number): CheckerStep[] {
  return notation.split(' ').flatMap((part) => {
    const pair = /^(bar|[1-9]|1\d|2[0-4])\/(off|[1-9]|1\d|2[0-4])$/.exec(part);
    if (!pair) return [];
    const from = pair[1] === 'bar' ? 25 : Number(pair[1]);
    const to = pair[2] === 'off' ? -1 : Number(pair[2]);
    return [
      {
        from: player === 0 ? from : 25 - from,
        to: player === 0 ? to : 25 - to,
      },
    ];
  });
}

/** Called only for a successful action, never for loading/resuming a session. */
export function soundsForAction(
  before: SoundState,
  after: SoundState,
  action: PlayAction,
): PlaySound[] {
  if (before.id !== after.id || after.version <= before.version) return [];
  const sounds: PlaySound[] = [];
  if (action === 'roll' || action === 'opening_roll') sounds.push('roll');
  let board = before.position.board;
  const records = after.history.slice(before.history.length);
  for (const [index, record] of records.entries()) {
    const steps = nativeSteps(record.notation, record.player);
    const bot = record.player !== before.human_player;
    if (
      bot &&
      record.dice.length === 2 &&
      (steps.length || record.notation === 'pass')
    ) {
      // These dice were already heard/shown at the opening-result checkpoint.
      if (!(action === 'start_opening_move' && index === 0))
        sounds.push('roll');
    }
    for (const step of steps) {
      if (bot) sounds.push(checkerSound(board, step, record.player));
      board = applySteps(board, [step], record.player);
    }
  }
  return sounds;
}
