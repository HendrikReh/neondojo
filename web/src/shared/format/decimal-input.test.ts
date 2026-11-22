import { describe, expect, it } from 'vitest';
import {
  canonicalDecimal,
  decimalSeparator,
  displayDecimal,
  steppedDecimal,
} from './decimal-input';

describe('decimal input', () => {
  it('uses the app language separator in every catalog language', () => {
    expect(
      Object.fromEntries(
        ['en', 'de', 'es', 'fr', 'it', 'pl'].map((locale) => [
          locale,
          decimalSeparator(locale),
        ]),
      ),
    ).toEqual({ en: '.', de: ',', es: ',', fr: ',', it: ',', pl: ',' });
  });

  it('shows stored values in the app language and stores typed text with a dot', () => {
    expect(displayDecimal('0.05', ',')).toBe('0,05');
    expect(displayDecimal('0.05', '.')).toBe('0.05');
    expect(canonicalDecimal(' 0,05 ')).toBe('0.05');
    expect(canonicalDecimal('0.05')).toBe('0.05');
    expect(canonicalDecimal('')).toBe('');
  });

  it('steps like a number input without rounding noise and within bounds', () => {
    expect(steppedDecimal('0.1', 1, { step: 0.005 })).toBe('0.105');
    expect(steppedDecimal('0.3', -1, { step: 0.1 })).toBe('0.2');
    expect(steppedDecimal('', 1, { step: 0.01 })).toBe('0.01');
    expect(steppedDecimal('0', -1, { step: 0.01, min: 0 })).toBe('0');
    expect(steppedDecimal('0.999', 1, { step: 0.01, max: 1 })).toBe('1');
    expect(steppedDecimal('0.0123', 1, { step: 0.005 })).toBe('0.0173');
    expect(steppedDecimal('abc', 1, { step: 0.01 })).toBeUndefined();
  });
});
