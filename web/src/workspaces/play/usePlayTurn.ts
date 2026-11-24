import { useMemo, useState } from 'react';
import {
  applySteps,
  matchingCheckerTurns,
  nextCheckerSteps,
  type CheckerStepWithDie,
} from '../../shared/board/play-state';
import { checkerFrame, reverseFrame } from '../../shared/board/play-animation';
import { unlockPlayAudio } from '../../shared/board/play-audio';
import type { PlaySession } from './play-api';
import type { usePlayAnimation } from './usePlayAnimation';
import type { usePlayReview } from './usePlayReview';

export type PlayTurn = ReturnType<typeof usePlayTurn>;

/** The unconfirmed checker turn: its steps, what may follow and the dice used. */
export function usePlayTurn({
  session,
  canMove,
  playback,
  review,
  isLocked,
}: {
  session: PlaySession | null;
  canMove: boolean;
  playback: ReturnType<typeof usePlayAnimation>;
  review: ReturnType<typeof usePlayReview>;
  isLocked: () => boolean;
}) {
  const [steps, setSteps] = useState<CheckerStepWithDie[]>([]);
  const [hovered, setHovered] = useState<number | null>(null);
  const [swapped, setSwapped] = useState(false);
  const savedDice = session?.position.dice;
  const dice = useMemo(
    () => (savedDice && swapped ? [...savedDice].reverse() : (savedDice ?? [])),
    [savedDice, swapped],
  );
  const moves = session?.legal_moves.length ? (session.legal_turns ?? []) : [];
  const continuations = canMove ? matchingCheckerTurns(moves, steps) : [];
  const choices = nextCheckerSteps(
    continuations,
    steps.length,
    dice[steps.length],
  );
  const hoverStep = hovered === null ? undefined : choices.get(hovered);
  const completed = continuations.find(
    (move) => move.steps.length === steps.length && steps.length > 0,
  );
  const forcedPass = canMove && session?.legal_moves.length === 0;
  const isDouble = dice[0] === dice[1];
  const canSwap =
    canMove &&
    !forcedPass &&
    !playback.animating &&
    steps.length === 0 &&
    dice.length === 2 &&
    !isDouble;
  return {
    steps,
    dice,
    sources: [...choices.keys()],
    destinations: hoverStep ? [hoverStep.to] : [],
    completed,
    forcedPass,
    canSwap,
    missingCheckerPaths:
      canMove && !moves.length && !!session?.legal_moves.length,
    usedDice: isDouble
      ? [Math.min(1, steps.length / 2), Math.max(0, (steps.length - 2) / 2)]
      : dice.map((die) => steps.some((step) => step.die === die)),
    hover: setHovered,
    reset: () => {
      setSteps([]);
      setHovered(null);
      setSwapped(false);
    },
    swap: () => {
      if (!canSwap || review.isActive()) return;
      setSwapped((value) => !value);
      setHovered(null);
    },
    point: (point: number) => {
      const step = choices.get(point);
      if (!session || !step || review.isActive()) return;
      unlockPlayAudio();
      const board = applySteps(
        session.position.board,
        steps,
        session.human_player,
      );
      setSteps((old) => [...old, step]);
      setHovered(point);
      void playback.play([
        checkerFrame(board, step, session.human_player, dice),
      ]);
    },
    undo: () => {
      if (
        !session ||
        !steps.length ||
        isLocked() ||
        playback.isPlaying() ||
        review.isActive()
      )
        return;
      const index = steps.length - 1;
      const board = applySteps(
        session.position.board,
        steps.slice(0, index),
        session.human_player,
      );
      const frame = reverseFrame(
        checkerFrame(board, steps[index], session.human_player, dice),
      );
      setSteps(steps.slice(0, index));
      setHovered(null);
      void playback.play([frame]);
    },
  };
}
