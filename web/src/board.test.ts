import { describe, expect, it } from 'vitest';
import { pipCounts, checkerCounts, matchContext } from './boardMath';

describe('board arithmetic', () => {
  it('counts starting pips from both player perspectives and includes bars', () => {
    const board = Array<number>(26).fill(0);
    board[24] = 2;
    board[13] = 5;
    board[8] = 3;
    board[6] = 5;
    board[1] = -2;
    board[12] = -5;
    board[17] = -3;
    board[19] = -5;
    expect(pipCounts(board)).toEqual([167, 167]);
    expect(checkerCounts(board)).toEqual([15, 15]);
    board[24]--;
    board[25]++;
    board[1]++;
    board[0]--;
    expect(pipCounts(board)).toEqual([168, 168]);
  });
  it('decodes score and cube ownership from the analysis match context', () => {
    expect(matchContext('QYkqASAAIAAE')).toMatchObject({
      length: 9,
      jacoby: false,
      scores: [2, 4],
      cube: 2,
      owner: 0,
      crawford: false,
    });
    expect(matchContext('cIkFAAAAAAAA')).toMatchObject({
      length: 0,
      jacoby: true,
    });
    expect(matchContext('cInlAAAAAAAE')).toMatchObject({
      scores: [0, 0],
      cube: 1,
      owner: null,
      crawford: false,
    });
  });
});
