import { describe, expect, it } from 'vitest';
import { moveDifferences, type PlayFacts } from './move-explanation';

const facts = (overrides: Partial<PlayFacts> = {}): PlayFacts => ({
  blots: [],
  hitting_rolls: 0,
  hit_pips: 0,
  deep_checkers: 0,
  stacked_checkers: 0,
  points: [6, 8, 13, 24],
  home_points: 1,
  longest_prime: 1,
  opponent_on_bar: 0,
  pips: 160,
  opponent_pips: 167,
  ...overrides,
});

describe('move differences', () => {
  it('lists what speaks for the best play first', () => {
    const differences = moveDifferences({
      best: facts({ points: [6, 7, 8, 13, 24], longest_prime: 3 }),
      played: facts({ blots: [2, 5], hitting_rolls: 21, hit_pips: 453 }),
    });
    expect(differences).toEqual([
      {
        kind: 'shots',
        best: 0,
        played: 21,
        bestCost: 0,
        playedCost: 453 / 36,
        favors: 'best',
      },
      { kind: 'pointsBest', points: [7], favors: 'best' },
      { kind: 'prime', best: 3, played: 1, favors: 'best' },
    ]);
  });
  it('judges shots by the pips at stake, not by the number of rolls', () => {
    // Splitting to the opponent's bar point is hit by every roll but cheaply;
    // two blots in the home board are hit less often and cost far more.
    const [shots, ...rest] = moveDifferences({
      best: facts({ blots: [11, 18, 24], hitting_rolls: 36, hit_pips: 180 }),
      played: facts({
        blots: [2, 4, 8],
        hitting_rolls: 20,
        hit_pips: 440,
        points: [6, 8, 13, 24],
        deep_checkers: 1,
      }),
    });
    expect(shots).toMatchObject({ kind: 'shots', favors: 'best' });
    expect(rest).toEqual([
      { kind: 'deep', best: 0, played: 1, favors: 'best' },
    ]);
  });
  it('puts winning chances first and concessions last', () => {
    expect(
      moveDifferences(
        {
          best: facts({ blots: [5], stacked_checkers: 1 }),
          played: facts({ blots: [5, 9], points: [6, 8, 13, 20, 24] }),
        },
        [0.6, 0.2, 0, 0.4, 0.1, 0],
        [0.55, 0.185, 0, 0.45, 0.105, 0],
      ),
    ).toEqual([
      { kind: 'win', best: 0.6, played: 0.55, favors: 'best' },
      { kind: 'blots', best: 1, played: 2, favors: 'best' },
      { kind: 'gammon', best: 0.2, played: 0.185, favors: 'best' },
      { kind: 'pointsPlayed', points: [20], favors: 'played' },
      { kind: 'stack', best: 1, played: 0, favors: 'played' },
    ]);
  });
  it('reports nothing for plays with the same facts', () => {
    expect(moveDifferences({ best: facts(), played: facts() })).toEqual([]);
  });
});
