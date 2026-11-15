import { expect, test } from 'vitest';
import {
  candidateEquityDifference,
  candidateLoss,
  isPlayedCandidate,
} from './candidate-comparison';

test('compares finite equity values in compatible units and reports signed differences', () => {
  const imported = { equity: -0.08, equity_kind: 'cubeful' };
  const local = { equity: -0.085, equity_kind: 'normalized_cubeful' };
  expect(candidateEquityDifference(local, imported)).toBeCloseTo(-0.005);
  expect(candidateEquityDifference(imported, local)).toBeCloseTo(0.005);
  expect(candidateEquityDifference(imported, imported)).toBe(0);
  expect(candidateEquityDifference(local, undefined)).toBeNull();
  expect(
    candidateEquityDifference({ ...local, equity: Number.NaN }, imported),
  ).toBeNull();
  expect(
    candidateEquityDifference({ ...local, equity_kind: 'mwc' }, imported),
  ).toBeNull();
});

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
