import { expect, test } from 'vitest';
import { candidateLoss, isPlayedCandidate } from './candidate-comparison';

test('loss uses the ranked best and preserves missing, unknown or incompatible values', () => {
  const best = { rank: 1, equity: 0.15, equity_kind: 'cubeful' };
  expect(candidateLoss({ ...best, equity: 0.11 }, best)).toBeCloseTo(0.04);
  expect(candidateLoss(best, best)).toBe(0);
  expect(candidateLoss({ ...best, equity: null }, best)).toBeNull();
  expect(candidateLoss({ ...best, equity_kind: 'unknown' }, best)).toBeNull();
  expect(candidateLoss({ ...best, equity_kind: 'mwc' }, best)).toBeNull();
  expect(candidateLoss({ ...best, equity: 0.2 }, best)).toBeNull();
  expect(candidateLoss(best, undefined)).toBeNull();
});

test('MWC differences retain probability units for percentage-point display', () => {
  expect(
    candidateLoss(
      { equity: 0.55, equity_kind: 'mwc' },
      { equity: 0.6, equity_kind: 'mwc' },
    ),
  ).toBeCloseTo(0.05);
});

test('played markers recognize equivalent legal notation and reject unprojectable alternatives', () => {
  const board = Array<number>(26).fill(0);
  board[24] = 1;
  const position = { board, dice: [6, 5], player: 0, notation: '24/18/13' };
  expect(isPlayedCandidate('24/13', position.notation, position)).toBe(true);
  expect(isPlayedCandidate('24/14', position.notation, position)).toBe(false);
  expect(isPlayedCandidate('24/13', position.notation)).toBe(false);
  expect(isPlayedCandidate('24/18/13', position.notation)).toBe(true);
});
