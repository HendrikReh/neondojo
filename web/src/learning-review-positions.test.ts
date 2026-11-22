import { describe, it, expect } from 'vitest';
import {
  buildLearningReviewPositions,
  reviewDecisionKey,
  type ReviewDecision,
} from './learning-review-positions';
import type {
  PlayReviewHistory,
  PlaySession,
} from './workspaces/play/play-api';
import type { Position } from './shared/api/api';
const position = (player = 0, ply = 1): Position => ({
  id: 's',
  match_uuid: 's',
  ordinal: ply - 1,
  game_no: 1,
  ply,
  player,
  dice: [3, 1],
  board: [
    0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0,
    2, 0,
  ],
  position_id: '4HPwATDgc/ABMA',
  gnubg_match_id: 'cInlAAAAAAAE',
  notation: '8/5 6/5',
});
const d = (
  n: number,
  kind = 'checker',
  checker_no: number | null = 1,
  player = 0,
  played = '8/5 6/5',
): ReviewDecision => ({
  decision_no: n,
  kind,
  checker_no,
  player,
  played,
  best: played,
  dice: kind === 'checker' ? [3, 1] : null,
  played_equity: 0,
  best_equity: 0,
  equity_loss: 0,
  severity: 'none',
  evaluation_status: 'evaluated',
});
function history(
  turns: Array<{
    game?: number;
    kind?: string;
    player?: number;
    notation?: string;
  }>,
): PlayReviewHistory {
  return {
    session_id: 's',
    version: 7,
    items: turns.map((t, index) => ({
      index,
      game_no: t.game ?? 1,
      move_no: index + 1,
      kind: t.kind ?? 'checker',
      before: {
        ...position(t.player ?? 0, index + 1),
        notation: t.notation ?? '8/5 6/5',
        dice: t.kind === 'cube' ? [] : [3, 1],
      },
      after: position(t.player ?? 0, index + 1),
    })),
  };
}
function review(
  games: Array<{ game_no: number; decisions: ReviewDecision[] }>,
) {
  return {
    games: games.map((g) => ({ ...g, engine: 'GNUbg' })),
  } as NonNullable<PlaySession['review']>;
}
describe('buildLearningReviewPositions', () => {
  it('maps by stored checker ordinal including opponent turns and repeated player moves', () => {
    const h = history([{ player: 0 }, { player: 1 }, { player: 0 }]);
    const r = review([
      {
        game_no: 1,
        decisions: [d(1), d(2, 'checker', 2, 1), d(3, 'checker', 3)],
      },
    ]);
    const mapped = buildLearningReviewPositions(r, h);
    expect(mapped.get('1:3')?.position).toEqual(h.items[2].before);
    expect(mapped.get('1:2')?.position.player).toBe(1);
    expect(mapped.get('1:1')?.historyIndex).toBe(0);
  });
  it('shares the exact pre-move board for no-double while hiding dice only for cube decisions', () => {
    const h = history([{}]);
    const original = structuredClone(h);
    const mapped = buildLearningReviewPositions(
      review([
        { game_no: 1, decisions: [d(1, 'cube', 1, 0, 'no double'), d(2)] },
      ]),
      h,
    );
    expect(mapped.get('1:1')?.position.board).toEqual(h.items[0].before.board);
    expect(mapped.get('1:1')?.position.dice).toEqual([]);
    expect(mapped.get('1:2')?.position.dice).toEqual([3, 1]);
    expect(h).toEqual(original);
  });
  it('distinguishes repeated actual double/take events using chronological cube context', () => {
    const h = history([
      { kind: 'cube', player: 0, notation: 'double' },
      { kind: 'cube', player: 1, notation: 'take' },
      { player: 0 },
      { kind: 'cube', player: 0, notation: 'double' },
      { kind: 'cube', player: 1, notation: 'take' },
      { player: 0 },
    ]);
    const decisions = [
      d(1, 'cube', null, 0, 'double'),
      d(2, 'cube', null, 1, 'take'),
      d(3),
      d(4, 'cube', null, 0, 'double'),
      d(5, 'cube', null, 1, 'take'),
      d(6, 'checker', 2),
    ];
    const mapped = buildLearningReviewPositions(
      review([{ game_no: 1, decisions }]),
      h,
    );
    expect(mapped.get('1:4')?.historyIndex).toBe(3);
    expect(mapped.get('1:5')?.historyIndex).toBe(4);
    expect(mapped.get('1:6')?.historyIndex).toBe(5);
  });
  it('keeps game boundaries independent even when checker numbers repeat', () => {
    const h = history([{ game: 1 }, { game: 2, player: 1 }]);
    const mapped = buildLearningReviewPositions(
      review([
        { game_no: 1, decisions: [d(1)] },
        { game_no: 2, decisions: [d(1, 'checker', 1, 1)] },
      ]),
      h,
    );
    expect(mapped.get(reviewDecisionKey(2, 1))?.historyIndex).toBe(1);
  });
  it('does not guess a starting board for legacy decisions without a checker ordinal', () => {
    expect(
      buildLearningReviewPositions(
        review([{ game_no: 1, decisions: [d(1, 'checker', null)] }]),
        history([{}]),
      ).size,
    ).toBe(0);
  });
  it('rejects a different player or dice instead of projecting from a wrong position', () => {
    const wrongDice = { ...d(1), dice: [6, 6] as [number, number] };
    for (const decision of [d(1, 'checker', 1, 1), wrongDice])
      expect(
        buildLearningReviewPositions(
          review([{ game_no: 1, decisions: [decision] }]),
          history([{}]),
        ).size,
      ).toBe(0);
  });
  it('accepts reversed dice order and forced passes on the exact stored turn', () => {
    const h = history([{}]);
    h.items[0].before.dice = [1, 3];
    const mapped = buildLearningReviewPositions(
      review([
        {
          game_no: 1,
          decisions: [
            {
              ...d(1),
              best: 'Cannot move',
              played: 'Cannot move',
              evaluation_status: 'forced',
            },
          ],
        },
      ]),
      h,
    );
    expect(mapped.get('1:1')?.position.dice).toEqual([1, 3]);
  });
  it('stops matching cube actions after an inconsistent source event', () => {
    const h = history([
      { kind: 'cube', player: 1, notation: 'drop' },
      { kind: 'cube', player: 0, notation: 'double' },
    ]);
    expect(
      buildLearningReviewPositions(
        review([{ game_no: 1, decisions: [d(1, 'cube', null, 0, 'double')] }]),
        h,
      ).size,
    ).toBe(0);
  });
  it('does not mutate the native report when choosing snapshots', () => {
    const r = review([{ game_no: 1, decisions: [d(1)] }]),
      before = structuredClone(r);
    buildLearningReviewPositions(r, history([{}]));
    expect(r).toEqual(before);
  });
});
