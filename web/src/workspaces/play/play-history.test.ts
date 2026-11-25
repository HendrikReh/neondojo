import { expect, it } from 'vitest';
import type { PlayDecision } from './play-api';
import { ratedDecisions } from './PlayHistory';

const decision = (
  id: string,
  history_index: number | undefined,
  original_action: string,
  first_loss: number | null = 0.1,
) =>
  ({
    id,
    history_index,
    original_action,
    first_loss,
  }) as unknown as PlayDecision;

it('rates a history row by its checker play, else by its first decision', () => {
  const rated = ratedDecisions([
    decision('double', 0, 'double'),
    decision('move', 0, 'move'),
    decision('later move', 0, 'move'),
    decision('take', 1, 'take'),
    decision('second take', 1, 'take'),
    decision('pass', 2, 'pass'),
    decision('cube after pass', 2, 'double'),
    decision('unrated', 3, 'move', null),
    decision('not played', undefined, 'move'),
  ]);
  expect([...rated].map(([index, entry]) => [index, entry.id])).toEqual([
    [0, 'move'],
    [1, 'take'],
    [2, 'pass'],
  ]);
  expect(ratedDecisions().size).toBe(0);
});
