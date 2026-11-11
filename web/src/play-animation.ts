import type { PlayAction, PlaySession } from './play-api';
import { applySteps, type CheckerStep } from './play-state';
import { checkerSound, nativeSteps, type PlaySound } from './play-sound-events';
export interface CheckerMotion extends CheckerStep {
  player: number;
}
interface BaseFrame {
  before: number[];
  player: number;
  dice: number[];
  sound?: PlaySound;
}
export interface CheckerFrame extends BaseFrame {
  kind: 'checker';
  after: number[];
  motions: CheckerMotion[];
}
export interface DiceFrame extends BaseFrame {
  kind: 'dice';
  roll: boolean;
  opening?: boolean;
  tied?: boolean;
}
export type PlayFrame = CheckerFrame | DiceFrame;
export type PlaybackFrame = PlayFrame & { id: number; rolling?: boolean };

export function checkerFrame(
  board: number[],
  step: CheckerStep,
  player: number,
  dice: number[],
): CheckerFrame {
  const sign = player === 0 ? 1 : -1;
  const motions: CheckerMotion[] = [{ ...step, player }];
  if (step.to >= 1 && step.to <= 24 && board[step.to] === -sign) {
    motions.push({
      from: step.to,
      to: player === 0 ? 0 : 25,
      player: 1 - player,
    });
  }
  return {
    kind: 'checker',
    before: [...board],
    after: applySteps(board, [step], player),
    player,
    dice: [...dice],
    motions,
    sound: checkerSound(board, step, player),
  };
}
export function reverseFrame(frame: CheckerFrame): CheckerFrame {
  return {
    ...frame,
    before: frame.after,
    after: frame.before,
    sound: undefined,
    motions: frame.motions.map(({ from, to, player }) => ({
      from: to,
      to: from,
      player,
    })),
  };
}
export function underlayBoard(frame: CheckerFrame): number[] {
  const board = [...frame.before];
  for (const motion of frame.motions)
    if (motion.from >= 0 && motion.from <= 25)
      board[motion.from] -= motion.player === 0 ? 1 : -1;
  return board;
}
export function actionFrames(
  before: PlaySession,
  after: PlaySession,
  action: PlayAction,
): PlayFrame[] {
  if (before.id !== after.id || after.version <= before.version) return [];
  if (action === 'opening_roll')
    return (after.opening?.rolls ?? []).map((dice) => ({
      kind: 'dice',
      before: before.position.board,
      player: dice[0] > dice[1] ? 0 : 1,
      dice,
      roll: true,
      opening: true,
      tied: dice[0] === dice[1],
      sound: 'roll',
    }));
  const frames: PlayFrame[] = [];
  if (action === 'roll')
    frames.push({
      kind: 'dice',
      before: before.position.board,
      player: before.human_player,
      dice: after.position.dice,
      roll: true,
      sound: 'roll',
    });
  let board = before.position.board;
  for (const [index, record] of after.history
    .slice(before.history.length)
    .entries()) {
    const steps = nativeSteps(record.notation, record.player),
      bot = record.player !== before.human_player;
    if (
      bot &&
      record.dice.length === 2 &&
      (steps.length || record.notation === 'pass')
    ) {
      const roll = !(action === 'start_opening_move' && index === 0);
      frames.push({
        kind: 'dice',
        before: board,
        player: record.player,
        dice: record.dice,
        roll,
        sound: roll ? 'roll' : undefined,
      });
    }
    for (const step of steps) {
      const frame = checkerFrame(board, step, record.player, record.dice);
      if (bot) frames.push(frame);
      board = frame.after;
    }
  }
  return frames;
}
