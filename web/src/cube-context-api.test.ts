import { describe, expect, it } from 'vitest';
import {
  properCubeOutcome,
  tooGoodToDouble,
  type CubeEvaluation,
} from './cube-context-api';

const offer = (
  no_double: number,
  double_take: number,
  double_pass: number,
): CubeEvaluation => ({
  unit: 'cubeful',
  no_double,
  double_take,
  double_pass,
});
const negate = (e: CubeEvaluation): CubeEvaluation => ({
  ...e,
  no_double: -e.no_double,
  double_take: -e.double_take,
  double_pass: -e.double_pass,
});

describe('cube outcomes', () => {
  it.each([
    ['no double', offer(0.3, 0.1, 1), 'no_double', false],
    ['double, take', offer(0.5, 0.7, 1), 'double_take', false],
    ['double, pass', offer(0.9, 1.3, 1), 'double_pass', false],
    ['too good', offer(1.2, 1.6, 1), 'no_double', true],
  ] as const)('%s', (_name, evaluation, proper, tooGood) => {
    expect(properCubeOutcome(evaluation, false)).toBe(proper);
    expect(tooGoodToDouble(evaluation, false)).toBe(tooGood);
    // A responder's record holds the same decision with negated values.
    expect(properCubeOutcome(negate(evaluation), true)).toBe(proper);
    expect(tooGoodToDouble(negate(evaluation), true)).toBe(tooGood);
  });
});
