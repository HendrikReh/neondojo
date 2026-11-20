import { describe, expect, it } from 'vitest';
import {
  matchingCheckerTurns,
  nextCheckerSteps,
  applySteps,
  matchingMoves,
  nextPoints,
  boardFlips,
  relativePoint,
} from './play-state';
const moves = [
  {
    notation: '8/5 6/5',
    steps: [
      { from: 8, to: 5 },
      { from: 6, to: 5 },
    ],
  },
  {
    notation: '8/5 24/23',
    steps: [
      { from: 8, to: 5 },
      { from: 24, to: 23 },
    ],
  },
];
describe('playing checker selection', () => {
  it('keeps every complete native continuation and never enables an incomplete turn', () => {
    const prefix = [{ from: 8, to: 5 }];
    expect(matchingMoves(moves, prefix)).toHaveLength(2);
    expect(nextPoints(moves, prefix, null)).toEqual([6, 24]);
    expect(nextPoints(moves, prefix, 6)).toEqual([5]);
    expect(matchingMoves(moves, [...prefix, { from: 6, to: 5 }])).toEqual([
      moves[0],
    ]);
    expect(nextPoints(moves, [...prefix, { from: 6, to: 5 }], null)).toEqual(
      [],
    );
  });
  it('previews hits, bar entries and bearing off without changing saved boards', () => {
    const board = Array(26).fill(0);
    board[25] = 1;
    board[24] = -1;
    board[1] = 1;
    const after = applySteps(
      board,
      [
        { from: 25, to: 24 },
        { from: 1, to: -1 },
      ],
      0,
    );
    expect(after[25]).toBe(0);
    expect(after[0]).toBe(-1);
    expect(after[24]).toBe(1);
    expect(after[1]).toBe(0);
    expect(board[24]).toBe(-1);
    const black = Array(26).fill(0);
    black[24] = -1;
    expect(applySteps(black, [{ from: 24, to: 26 }], 1)[24]).toBe(0);
  });
  it('fixes perspective to the human and reflects geometry for both directions', () => {
    expect(boardFlips(0, 'right')).toEqual([1, 1]);
    expect(boardFlips(0, 'left')).toEqual([-1, 1]);
    expect(boardFlips(1, 'right')).toEqual([1, -1]);
    expect(boardFlips(1, 'left')).toEqual([-1, -1]);
    expect(relativePoint(24, 1)).toBe(1);
    expect(relativePoint(1, 0)).toBe(1);
  });
});

it('prefers the displayed die among server paths and keeps the chosen die in the prefix', () => {
  const turns = [
    {
      steps: [
        { from: 13, to: 10 },
        { from: 10, to: 8 },
      ],
      dice: [3, 2],
      move_index: 0,
    },
    {
      steps: [
        { from: 13, to: 11 },
        { from: 11, to: 8 },
      ],
      dice: [2, 3],
      move_index: 0,
    },
  ];
  expect(nextCheckerSteps(turns, 0, 3).get(13)).toEqual({
    from: 13,
    to: 10,
    die: 3,
  });
  const step = { from: 13, to: 11, die: 2 };
  expect(nextCheckerSteps(turns, 0, 2).get(13)).toEqual(step);
  expect(matchingCheckerTurns(turns, [step])).toEqual([turns[1]]);
  expect(matchingCheckerTurns(turns, [{ ...step, die: 3 }])).toEqual([]);
});
