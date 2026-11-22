/**
 * A performance rating is an average over a few hundred decisions at most,
 * so one decimal is all the precision it carries.
 */
export const prDigits = {
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
} as const satisfies Intl.NumberFormatOptions;
