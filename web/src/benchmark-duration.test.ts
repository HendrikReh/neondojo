import { expect, it } from 'vitest';
import { formatBenchmarkDuration } from './benchmark-duration';

const format = (locale: string) => (ms: number) =>
  formatBenchmarkDuration(ms, (value, options) =>
    new Intl.NumberFormat(locale, options).format(value),
  );

it('keeps short totals precise and shortens long totals', () => {
  const en = format('en');
  expect(en(999.5)).toBe('999.5 ms');
  expect(en(1608.53)).toBe('1.61s');
  expect(en(60_000)).toBe('1m');
  expect(en(1_933_872.5)).toBe('32m 14s');
  expect(en(3_599_600)).toBe('1h');
  expect(en(3_900_000)).toBe('1h 5m');
  expect(en(7_199_000)).toBe('2h');
});

it('uses the locale for decimals and units', () => {
  expect(format('de')(20_225.88)).toMatch(/^20,23\s?\S+$/);
});
