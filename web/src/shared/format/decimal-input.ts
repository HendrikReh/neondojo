import type { KeyboardEvent } from 'react';
import { useI18n } from '../i18n/i18n';
import { numberFormat } from '../i18n/i18n-core';

/** The decimal separator of an app language. */
export function decimalSeparator(locale: string) {
  return (
    numberFormat(locale)
      .formatToParts(1.5)
      .find((part) => part.type === 'decimal')?.value ?? '.'
  );
}

/** Typed text in the dot-decimal form callers store and parse. */
export function canonicalDecimal(text: string) {
  return text.trim().replace(',', '.');
}

export function displayDecimal(value: string, separator: string) {
  return separator === '.' ? value : value.replace('.', separator);
}

const decimals = (value: string) => value.split('.')[1]?.length ?? 0;

/** The value one step up or down, clamped and without binary rounding noise. */
export function steppedDecimal(
  value: string,
  direction: 1 | -1,
  { step, min, max }: { step: number; min?: number; max?: number },
) {
  const current = value.trim() === '' ? 0 : Number(value);
  if (!Number.isFinite(current)) return undefined;
  const places = Math.max(decimals(String(step)), decimals(value.trim()));
  let next = Number((current + direction * step).toFixed(places));
  if (min !== undefined) next = Math.max(min, next);
  if (max !== undefined) next = Math.min(max, next);
  return String(next);
}

/**
 * A decimal field in the app language. Native number inputs show the
 * operating system's separator, which can contradict the numbers the app
 * formats around them. The field accepts either separator, keeps arrow-key
 * steps and the spinbutton role, and passes callers the same dot-decimal
 * string a number input did.
 */
export function useDecimalInput({
  value,
  onChange,
  step,
  min,
  max,
  onKeyDown,
}: {
  value: string;
  onChange: (value: string) => void;
  step: number;
  min?: number;
  max?: number;
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
}) {
  const { locale } = useI18n();
  const number = value.trim() === '' ? NaN : Number(value);
  return {
    inputProps: {
      type: 'text',
      inputMode: 'decimal' as const,
      autoComplete: 'off',
      // Arrow keys step the value, so it keeps a number input's role.
      role: 'spinbutton',
      'aria-valuemin': min,
      'aria-valuemax': max,
      'aria-valuenow': Number.isFinite(number) ? number : undefined,
      value: displayDecimal(value, decimalSeparator(locale)),
      onKeyDown: (event: KeyboardEvent<HTMLInputElement>) => {
        onKeyDown?.(event);
        if (
          event.defaultPrevented ||
          (event.key !== 'ArrowUp' && event.key !== 'ArrowDown')
        )
          return;
        const next = steppedDecimal(value, event.key === 'ArrowUp' ? 1 : -1, {
          step,
          min,
          max,
        });
        if (next === undefined) return;
        event.preventDefault();
        onChange(next);
      },
    },
    change: (text: string) => onChange(canonicalDecimal(text)),
  };
}
