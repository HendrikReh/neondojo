import { describe, expect, it } from 'vitest';
import { moveDifferences, type PlayFacts } from './move-explanation';

const facts = (overrides: Partial<PlayFacts> = {}): PlayFacts => ({
  blots: [],
  hitting_rolls: 0,
  points: [6, 8, 13, 24],
  home_points: 1,
  longest_prime: 1,
  opponent_on_bar: 0,
  pips: 160,
  opponent_pips: 167,
  ...overrides,
});

describe('move differences', () => {
  it('lists shots and points before structure', () => {
    const differences = moveDifferences({
      best: facts({ points: [6, 7, 8, 13, 24], longest_prime: 3 }),
      played: facts({ blots: [2, 5], hitting_rolls: 21 }),
    });
    expect(differences).toEqual([
      { kind: 'shots', best: 0, played: 21 },
      { kind: 'pointsBest', points: [7] },
      { kind: 'prime', best: 3, played: 1 },
    ]);
  });
  it('mentions blots only when the shots are equal, and gammons from one point', () => {
    expect(
      moveDifferences(
        { best: facts({ blots: [5] }), played: facts({ blots: [5, 9] }) },
        [0.6, 0.2, 0, 0.4, 0.1, 0],
        [0.6, 0.185, 0, 0.4, 0.105, 0],
      ),
    ).toEqual([
      { kind: 'blots', best: 1, played: 2 },
      { kind: 'gammon', best: 0.2, played: 0.185 },
    ]);
  });
  it('reports nothing for plays with the same facts', () => {
    expect(moveDifferences({ best: facts(), played: facts() })).toEqual([]);
  });
});
