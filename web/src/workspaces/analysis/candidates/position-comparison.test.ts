import { expect, test } from 'vitest';
import type { ComparisonCandidate } from './candidate-comparison';
import {
  positionComparisonOptions,
  positionComparisonRows,
  sortPositionComparisonRows,
} from './position-comparison';
import { entryForJob } from '../analysis-catalog-test-fixtures';
import type { Job } from '../../../shared/api/api';

const candidate = (
  notation: string,
  rank: number,
  equity: number,
  depth: string,
): ComparisonCandidate => ({
  notation,
  rank,
  equity,
  depth,
  equity_kind: 'cubeful',
  probabilities: [],
});

test('prefers the latest deepest result with the right engine and scope', () => {
  const entry = (id: string, plies: number, game: number, created_at: string) =>
    entryForJob({ id, plies, created_at, status: 'completed' } as Job, game);
  const later = '2026-11-15T09:05:00Z';
  const earlier = '2026-11-15T09:00:00Z';
  const entries = [
    entry('newer-other-game', 4, 2, later),
    entry('newest-shallow', 2, 1, later),
    entry('older-deep', 4, 1, earlier),
    entry('newer-deep', 4, 1, later),
  ];
  expect(
    positionComparisonOptions(
      entries,
      { id: 'position', game_no: 1 },
      'gnubg',
    ).map((option) => option.id),
  ).toEqual(['job:newer-deep', 'job:older-deep', 'job:newest-shallow']);
  expect(
    positionComparisonOptions(
      entries,
      { id: 'position', game_no: 1 },
      'bgsage',
    ),
  ).toEqual([]);
});

test('aligns end positions and preserves the rank, equity and depth of each engine', () => {
  const board = Array<number>(26).fill(0);
  board[24] = 2;
  board[13] = 5;
  board[8] = 3;
  board[6] = 5;
  const position = { board, player: 0, dice: [6, 3], notation: '13/10 13/7' };
  const xgPlayed = candidate('13/10 13/7', 2, -0.08, 'XGRoller+');
  const xgSplit = candidate('24/21 21/15', 3, -0.005, '4-ply');
  const gnuCombined = candidate('24/15', 2, -0.007, '2-ply');
  const sageCombined = candidate('24/15', 2, -0.004, '4-ply');
  const gnuOnly = candidate('8/5 8/2', 14, -0.36, '0-ply');
  const rows = positionComparisonRows(position, [
    [candidate('24/18 13/10', 1, 0.012, 'XGRoller+'), xgPlayed, xgSplit],
    [gnuCombined, gnuOnly],
    [sageCombined],
  ]);

  expect(rows).toHaveLength(4);
  expect(rows[1]).toMatchObject({
    xgEquityRank: 2,
    candidates: [xgSplit, gnuCombined, sageCombined],
  });
  expect(rows[2]).toMatchObject({
    played: true,
    xgEquityRank: 3,
    candidates: [xgPlayed, undefined, undefined],
  });
  expect(rows[3].candidates).toEqual([undefined, gnuOnly, undefined]);
});

test('sorts by the selected engine’s absolute XG difference, with missing values last', () => {
  const rows = [
    [0.5, 0.5, 0.75],
    [0.25, 0.125, 0.25],
    [0.125, 0.25, 0.25],
    [0, -0.25, 0],
    [undefined, 0.875, 0.875],
    [undefined, 0.75, 0.75],
  ].map((equities, index) => ({
    key: String(index),
    notation: '24/18 13/10',
    played: false,
    candidates: equities.map((equity) =>
      equity === undefined
        ? undefined
        : candidate('24/18 13/10', index + 1, equity, '4-ply'),
    ),
  }));

  expect(
    sortPositionComparisonRows(rows, 'gnubg').map((row) => row.key),
  ).toEqual(['3', '1', '2', '0', '4', '5']);
  expect(
    sortPositionComparisonRows(rows, 'bgsage').map((row) => row.key),
  ).toEqual(['0', '2', '1', '3', '4', '5']);
  expect(rows.map((row) => row.key)).toEqual(['0', '1', '2', '3', '4', '5']);
});
