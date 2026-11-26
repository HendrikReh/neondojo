import { describe, expect, it } from 'vitest';
import { decisionFilters } from './DecisionSearch';

const draft = {
  player: 'Alice',
  database: '',
  kind: 'cube_response',
  mistake: '',
  plan: '',
  minLoss: '0.05',
  maxLoss: '',
  away: '2',
  opponentAway: '',
  cubeValue: '',
  cubeOwner: 'centered',
};

describe('decision search filters', () => {
  it('sends only the filters that were filled in', () => {
    expect(decisionFilters(draft, 0.02)).toEqual({
      player: 'Alice',
      database_id: undefined,
      kind: 'cube_response',
      mistake_kind: undefined,
      game_plan: undefined,
      cube_owner: 'centered',
      min_loss: 0.05,
      max_loss: undefined,
      away: 2,
      opponent_away: undefined,
      cube_value: undefined,
      limit: 50,
    });
  });
  it('waits for a player and for usable numbers', () => {
    expect(decisionFilters({ ...draft, player: '' }, 0.02)).toBeUndefined();
    expect(decisionFilters({ ...draft, minLoss: 'abc' }, 0.02)).toBeUndefined();
  });
});
