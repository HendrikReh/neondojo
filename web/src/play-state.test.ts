import { describe, expect, it } from 'vitest';
import {
  orderedLegalTurns,
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

describe('first-die interaction paths backed by native complete legal plays', () => {
  it('splits combined native runs into dice steps and permits either selected die order', () => {
    const b = Array(26).fill(0);
    b[13] = 1;
    const native = [{ notation: '13/8', steps: [{ from: 13, to: 8 }] }];
    expect(orderedLegalTurns(b, 0, [3, 2], native).map((t) => t.steps)).toEqual(
      [
        [
          { from: 13, to: 10 },
          { from: 10, to: 8 },
        ],
      ],
    );
    expect(orderedLegalTurns(b, 0, [2, 3], native).map((t) => t.steps)).toEqual(
      [
        [
          { from: 13, to: 11 },
          { from: 11, to: 8 },
        ],
      ],
    );
  });
  it('finds alternate checker order for the same native result, without inventing another final board', () => {
    const b = Array(26).fill(0);
    b[8] = 1;
    b[6] = 1;
    const native = [
      {
        notation: '8/5 6/5',
        steps: [
          { from: 8, to: 5 },
          { from: 6, to: 5 },
        ],
      },
    ];
    expect(orderedLegalTurns(b, 0, [1, 3], native)).toContainEqual({
      steps: [
        { from: 6, to: 5 },
        { from: 8, to: 5 },
      ],
      dice: [1, 3],
      moveIndex: 0,
      notation: '8/5 6/5',
    });
    expect(
      orderedLegalTurns(b, 0, [3, 1], native).every((t) => t.moveIndex === 0),
    ).toBe(true);
  });
  it('requires swapping a blocked first die rather than silently playing the second', () => {
    const b = Array(26).fill(0);
    b[25] = 1;
    b[22] = -2;
    const native = [
      {
        notation: 'bar/23 23/20',
        steps: [
          { from: 25, to: 23 },
          { from: 23, to: 20 },
        ],
      },
    ];
    expect(orderedLegalTurns(b, 0, [3, 2], native)).toEqual([]);
    expect(orderedLegalTurns(b, 0, [2, 3], native)).toHaveLength(1);
  });
  it('enforces maximum usage and the higher die when only one non-winning move can be played', () => {
    const b = Array(26).fill(0);
    b[25] = 1;
    b[6] = 1;
    b[17] = -2;
    b[4] = -2;
    const native = [{ notation: 'bar/19', steps: [{ from: 25, to: 19 }] }];
    expect(orderedLegalTurns(b, 0, [2, 6], native)).toEqual([]);
    expect(orderedLegalTurns(b, 0, [6, 2], native)).toEqual([
      {
        steps: [{ from: 25, to: 19 }],
        dice: [6],
        moveIndex: 0,
        notation: 'bar/19',
      },
    ]);
  });
  it('allows a native winning bearoff immediately, even if using the smaller die first would take two steps', () => {
    const b = Array(26).fill(0);
    b[2] = 1;
    expect(
      orderedLegalTurns(
        b,
        0,
        [2, 1],
        [{ notation: '2/off', steps: [{ from: 2, to: -1 }] }],
      ),
    ).toEqual([
      {
        steps: [{ from: 2, to: -1 }],
        dice: [2],
        moveIndex: 0,
        notation: '2/off',
      },
    ]);
  });
  it('uses four individual dice on doubles and mirrors black entry/bearoff correctly', () => {
    const b = Array(26).fill(0);
    b[13] = 1;
    const native = [{ notation: '13/1', steps: [{ from: 13, to: 1 }] }];
    const [turn] = orderedLegalTurns(b, 0, [3, 3], native);
    expect(turn.steps).toEqual([
      { from: 13, to: 10 },
      { from: 10, to: 7 },
      { from: 7, to: 4 },
      { from: 4, to: 1 },
    ]);
    expect(turn.dice).toEqual([3, 3, 3, 3]);
    const black = Array(26).fill(0);
    black[0] = -1;
    black[24] = -1;
    const result = orderedLegalTurns(
      black,
      1,
      [1, 1],
      [
        {
          notation: 'bar/1 1/4',
          steps: [
            { from: 0, to: 1 },
            { from: 1, to: 4 },
          ],
        },
      ],
    );
    expect(
      result.some((t) => t.steps[0].from === 0 && t.steps[0].to === 1),
    ).toBe(true);
  });
});
