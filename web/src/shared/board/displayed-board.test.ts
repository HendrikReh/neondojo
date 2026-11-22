import { expect, it } from 'vitest';
import { displayedBoardPosition } from './displayed-board';
import type { Position } from '../http/api';
import { projectMove } from './move-preview';

it('shares the actual board and dice calculation for Initial, Arrows and Final', () => {
  const board = Array<number>(26).fill(0);
  board[13] = 1;
  board[1] = -1;
  const position = {
    board,
    player: 0,
    dice: [6, 1],
    notation: '13/6',
  } as Position;
  const preview = projectMove(position);
  expect(preview.ok).toBe(true);
  expect(displayedBoardPosition(position, preview, 'before')).toBe(position);
  expect(displayedBoardPosition(position, preview, 'arrows')).toBe(position);
  const final = displayedBoardPosition(position, preview, 'after');
  expect(final.board[13]).toBe(0);
  expect(final.board[6]).toBe(1);
  expect(final.dice).toEqual([]);
  expect(position.board[13]).toBe(1);
  expect(
    displayedBoardPosition(
      position,
      { ok: false, reason: 'bad notation' },
      'after',
    ),
  ).toBe(position);
});
