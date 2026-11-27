/** Shared JSON boundary validation type guards. */
export const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

export const isNonEmptyString = (value: unknown): value is string =>
  typeof value === 'string' && value.trim().length > 0;

export const isPositiveFinite = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value) && value > 0;

export const isNonNegativeFinite = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value) && value >= 0;

export const isSafeIntegerAtLeast = (
  value: unknown,
  minimum = 0,
): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= minimum;
