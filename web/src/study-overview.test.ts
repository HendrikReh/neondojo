import { describe, expect, it } from 'vitest';
import type { MistakeGroup } from './recurring-mistakes-api';
import {
  defaultStudyPlayer,
  rankMistakeGroups,
  recommendStudyAction,
} from './study-overview';

const rates = {
  decision_count: 50,
  error_rate: 0.1,
  loss_per_100: 1,
  small_sample: false,
  trend: null,
};
const checker: MistakeGroup = {
  game_plan: 'priming',
  mistake_kind: 'checker',
  count: 5,
  match_count: 2,
  total_loss: 0.5,
  rates,
};
const cube: MistakeGroup = {
  ...checker,
  mistake_kind: 'missed_double',
  match_count: 3,
};
const deck = { id: 'deck-1', name: 'My deck' };

describe('study overview', () => {
  it('matches the full configured player name and preserves the canonical spelling', () => {
    expect(defaultStudyPlayer('  alice  ', ['ALICE', 'Alice B.'])).toBe(
      'ALICE',
    );
    expect(defaultStudyPlayer('alice', ['ALICE', 'Alice'])).toBe('');
    expect(defaultStudyPlayer('alice', ['Alice B.'])).toBe('');
  });

  it('ranks by normalized loss first, keeps small samples visible and excludes single-match groups', () => {
    const single = { ...checker, total_loss: 20, match_count: 1 };
    const biggest = {
      ...checker,
      total_loss: 1,
      rates: { ...rates, decision_count: 40, loss_per_100: 2.5 },
    };
    const small = {
      ...checker,
      mistake_kind: 'wrong_pass' as const,
      rates: {
        decision_count: 5,
        error_rate: 1,
        loss_per_100: 10,
        small_sample: true,
        trend: null,
      },
    };
    const frequent = {
      ...checker,
      mistake_kind: 'wrong_take' as const,
      total_loss: 3,
      rates: {
        decision_count: 600,
        error_rate: 0.01,
        loss_per_100: 0.5,
        small_sample: false,
        trend: null,
      },
    };
    const more = { ...checker, count: 6 };
    const tied = { ...checker, game_plan: 'anchoring' as const };
    expect(
      rankMistakeGroups([
        checker,
        single,
        cube,
        tied,
        biggest,
        more,
        small,
        frequent,
      ]),
    ).toEqual([biggest, cube, more, tied, checker, frequent, small]);
  });

  it('never recommends a group with a small or undefined sample', () => {
    for (const sample of [null, { ...rates, small_sample: true }]) {
      expect(
        recommendStudyAction(
          { status: 'ready' },
          { status: 'ready', group: { ...checker, rates: sample } },
        ),
      ).toEqual({ kind: 'cards' });
    }
  });

  it('prioritizes due cards and uses a ready group if card counts fail', () => {
    const mistake = { status: 'ready', group: checker } as const;
    expect(recommendStudyAction({ status: 'loading' }, mistake)).toEqual({
      kind: 'loading',
    });
    expect(recommendStudyAction({ status: 'ready', deck }, mistake)).toEqual({
      kind: 'deck',
      deck,
    });
    expect(recommendStudyAction({ status: 'ready' }, mistake)).toEqual({
      kind: 'mistake',
      group: checker,
      rates,
    });
    expect(recommendStudyAction({ status: 'error' }, mistake)).toEqual({
      kind: 'mistake',
      group: checker,
      rates,
    });
  });

  it('distinguishes loading, choosing a player, an empty result and an unavailable result', () => {
    const cards = { status: 'ready' } as const;
    expect(recommendStudyAction(cards, { status: 'loading' })).toEqual({
      kind: 'loading',
    });
    expect(recommendStudyAction(cards, { status: 'choose-player' })).toEqual({
      kind: 'choose-player',
    });
    expect(recommendStudyAction(cards, { status: 'ready' })).toEqual({
      kind: 'cards',
    });
    expect(recommendStudyAction(cards, { status: 'error' })).toEqual({
      kind: 'unavailable',
    });
    expect(
      recommendStudyAction({ status: 'error' }, { status: 'ready' }),
    ).toEqual({ kind: 'unavailable' });
  });
});
