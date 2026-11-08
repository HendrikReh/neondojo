import { describe, expect, test } from 'vitest';
import { projectMove, type MovePreview } from './move-preview';

const board = (points: Record<number, number>) => {
  const result = Array<number>(26).fill(0);
  for (const [point, count] of Object.entries(points))
    result[Number(point)] = count;
  return result;
};
const position = (
  points: Record<number, number>,
  dice: number[],
  notation: string,
  player = 0,
) => ({ board: board(points), player, dice, notation });
const success = (result: MovePreview) => {
  expect(result.ok).toBe(true);
  if (!result.ok) throw new Error(result.reason);
  return result;
};

describe('projectMove', () => {
  test('projects a complete roll without changing the source position', () => {
    const source = position(
      { 24: 2, 13: 5, 8: 3, 6: 5, 1: -2, 12: -5, 17: -3, 19: -5 },
      [6, 1],
      '13/7 8/7',
    );
    const original = structuredClone(source);
    const preview = success(projectMove(source));
    expect(preview.board).toEqual(
      board({ 24: 2, 13: 4, 8: 2, 7: 2, 6: 5, 1: -2, 12: -5, 17: -3, 19: -5 }),
    );
    expect(preview.steps).toEqual([
      { from: 13, to: 7, die: 6, hit: false },
      { from: 8, to: 7, die: 1, hit: false },
    ]);
    expect(preview.forced).toBe(false);
    expect(source).toEqual(original);
    expect(preview.board).not.toBe(source.board);
  });

  test('honors player-relative coordinates for black and hits onto the white bar', () => {
    const preview = success(
      projectMove(position({ 0: -1, 1: 1, 5: -1 }, [1, 2], 'bar/24* 20/18', 1)),
    );
    expect(preview.board).toEqual(board({ 1: -1, 7: -1, 25: 1 }));
    expect(preview.steps).toEqual([
      { from: 'bar', to: 1, die: 1, hit: true },
      { from: 5, to: 7, die: 2, hit: false },
    ]);
  });

  test('finds legal execution order when notation lists the ordinary move before bar entry', () => {
    const preview = success(
      projectMove(position({ 25: 1, 13: 1, 24: -1 }, [1, 2], '13/11 bar/24*')),
    );
    expect(preview.steps[0]).toEqual({
      from: 'bar',
      to: 24,
      die: 1,
      hit: true,
    });
    expect(preview.board[0]).toBe(-1);
    expect(preview.board[11]).toBe(1);
  });

  test('expands a joined run through an unblocked intermediate point', () => {
    const preview = success(
      projectMove(position({ 24: 1, 18: -2 }, [6, 5], '24/13')),
    );
    expect(preview.steps).toEqual([
      { from: 24, to: 19, die: 5, hit: false },
      { from: 19, to: 13, die: 6, hit: false },
    ]);
  });

  test('rejects a joined run when every intermediate point is blocked', () => {
    expect(
      projectMove(position({ 24: 1, 18: -2, 19: -2 }, [6, 5], '24/13')).ok,
    ).toBe(false);
  });

  test('does not invent an intermediate hit omitted by joined notation', () => {
    expect(
      projectMove(position({ 24: 1, 18: -1, 19: -2 }, [6, 5], '24/13')).ok,
    ).toBe(false);
    const preview = success(
      projectMove(position({ 24: 1, 18: -1, 19: -2 }, [6, 5], '24/18*/13')),
    );
    expect(preview.steps[0].hit).toBe(true);
    expect(preview.board[0]).toBe(-1);
  });

  test('accepts omitted hit stars from XG candidate endpoints but rejects false stars', () => {
    expect(
      success(projectMove(position({ 8: 1, 7: -1 }, [1, 2], '8/7 7/5')))
        .board[0],
    ).toBe(-1);
    expect(projectMove(position({ 8: 1 }, [1, 2], '8/7* 7/5')).ok).toBe(false);
  });

  test('supports doubled multiplicity and all four dice', () => {
    const preview = success(
      projectMove(position({ 13: 2, 6: 2 }, [3, 3], '13/10(2) 6/3(2)')),
    );
    expect(preview.board).toEqual(board({ 10: 2, 3: 2 }));
    expect(preview.steps).toHaveLength(4);
  });

  test('keeps repeated tokens as separate moves rather than deduplicating them', () => {
    expect(
      projectMove(position({ 13: 1, 6: 2 }, [3, 3], '13/10 13/10 6/3(2)')).ok,
    ).toBe(false);
    expect(
      success(
        projectMove(position({ 13: 2, 6: 2 }, [3, 3], '13/10 13/10 6/3(2)')),
      ).steps,
    ).toHaveLength(4);
  });

  test('rejects incomplete rolls even when the remaining die becomes blocked', () => {
    // Entering with 1 gets stuck; entering with 2 permits 8/7 with the 1.
    expect(
      projectMove(position({ 25: 1, 8: 1, 22: -2, 6: -2 }, [2, 1], 'bar/24'))
        .ok,
    ).toBe(false);
  });

  test('requires the higher die when just one of two different dice can be used', () => {
    const source = position({ 25: 1, 22: -2 }, [2, 1], 'bar/24');
    expect(projectMove(source).ok).toBe(false);
    expect(success(projectMove(source, 'bar/23')).steps[0].die).toBe(2);
  });

  test('respects bar priority even for a plausible ordinary checker move', () => {
    expect(
      projectMove(position({ 25: 1, 8: 1, 24: -2, 23: -2 }, [1, 2], '8/7 7/5'))
        .ok,
    ).toBe(false);
  });

  test('rejects bear-off while any checker remains outside the home board', () => {
    expect(projectMove(position({ 8: 2, 6: 1 }, [6, 1], '6/off 8/7')).ok).toBe(
      false,
    );
    expect(
      success(projectMove(position({ 7: 1, 6: 1 }, [6, 1], '7/6 6/off')))
        .board[6],
    ).toBe(1);
  });

  test('only permits oversized bear-off from the farthest occupied home point', () => {
    expect(projectMove(position({ 6: 2, 3: 1 }, [5, 4], '3/off 6/2')).ok).toBe(
      false,
    );
    const preview = success(
      projectMove(position({ 3: 2 }, [6, 5], '3/off(2)')),
    );
    expect(preview.board).toEqual(board({}));
    expect(preview.steps.every((step) => step.to === 'off')).toBe(true);
  });

  test('bear-off never changes the opposing bar slot for either player', () => {
    expect(
      success(projectMove(position({ 1: 1, 0: -2 }, [1, 2], '1/off'))).board[0],
    ).toBe(-2);
    expect(
      success(projectMove(position({ 24: -1, 25: 2 }, [1, 2], '1/off', 1)))
        .board[25],
    ).toBe(2);
  });

  test('ends a game immediately when the final checker is borne off', () => {
    const preview = success(projectMove(position({ 6: 1 }, [6, 1], '6/off')));
    expect(preview.board).toEqual(board({}));
    expect(preview.steps).toEqual([{ from: 6, to: 'off', die: 6, hit: false }]);
  });

  test('allows a dance only when no die is playable', () => {
    const source = position({ 25: 1, 24: -2, 23: -2 }, [1, 2], 'Cannot move');
    expect(success(projectMove(source))).toEqual({
      ok: true,
      board: source.board,
      steps: [],
      forced: true,
    });
    expect(projectMove(position({ 25: 1, 24: -2 }, [1, 2], 'dance')).ok).toBe(
      false,
    );
  });

  test.each([
    '',
    'Double',
    'Take',
    'Resign',
    '24/13 ??',
    '25/24',
    'bar/off',
    '13/10(9)',
    '13//10',
    '24/13*foo',
  ])('does not project unsupported or malformed notation %j', (notation) => {
    expect(projectMove(position({ 24: 2, 13: 2 }, [6, 5], notation)).ok).toBe(
      false,
    );
  });

  test('rejects malformed boards, mover and dice rather than showing an invented after state', () => {
    const source = position({ 8: 1 }, [1, 2], '8/7 7/5');
    for (const invalid of [
      { ...source, board: [1] },
      { ...source, board: board({ 25: -1, 8: 1 }) },
      { ...source, board: board({ 8: 16 }) },
      { ...source, board: board({ 8: 1.5 }) },
      { ...source, player: 2 },
      { ...source, dice: [] },
      { ...source, dice: [7, 1] },
    ])
      expect(projectMove(invalid).ok).toBe(false);
  });
});
