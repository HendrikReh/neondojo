import type { components } from '../../src/generated-api';
import { nativeSteps } from '../../src/shared/board/play-sound-events';

export function checkerPath(
  dice: number[],
  notation: string,
  player = 0,
): components['schemas']['LiveCheckerTurn'] {
  return { dice, steps: nativeSteps(notation, player), move_index: 0 };
}

export function eitherOrder(dice: number[], notation: string, player = 0) {
  const path = checkerPath(dice, notation, player);
  return [
    path,
    { ...path, dice: [...dice].reverse(), steps: [...path.steps].reverse() },
  ];
}

export function joinedThirteenToEight(player = 0) {
  return [
    checkerPath([3, 2], '13/10 10/8', player),
    checkerPath([2, 3], '13/11 11/8', player),
  ];
}
