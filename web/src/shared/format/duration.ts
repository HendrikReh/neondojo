export type FormatNumber = (
  value: number,
  options?: Intl.NumberFormatOptions,
) => string;

export type DurationUnit = 'second' | 'minute' | 'hour';

export interface DecomposedDuration {
  major: number;
  majorUnit: DurationUnit;
  minor: number;
  minorUnit?: DurationUnit;
}

/** Decomposes a duration >= 1 second into at most two non-zero display units. */
export function decomposeDuration(ms: number): DecomposedDuration {
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) {
    return { major: seconds, majorUnit: 'second', minor: 0 };
  }
  if (seconds < 3600) {
    return {
      major: Math.floor(seconds / 60),
      majorUnit: 'minute',
      minor: seconds % 60,
      minorUnit: 'second',
    };
  }
  const minutes = Math.round(ms / 60_000);
  return {
    major: Math.floor(minutes / 60),
    majorUnit: 'hour',
    minor: minutes % 60,
    minorUnit: 'minute',
  };
}

export const benchmarkTimeDigits = {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
};

const unitOptions = (name: DurationUnit, digits = 0) =>
  ({
    style: 'unit',
    unit: name,
    unitDisplay: 'narrow',
    maximumFractionDigits: digits,
  }) satisfies Intl.NumberFormatOptions;

/** Milliseconds stay exact below one second; longer totals use readable units. */
export function formatBenchmarkDuration(
  milliseconds: number,
  formatNumber: FormatNumber,
): string {
  if (milliseconds < 1000)
    return `${formatNumber(milliseconds, { maximumFractionDigits: 2 })} ms`;
  if (milliseconds < 60_000)
    return formatNumber(milliseconds / 1000, unitOptions('second', 2));
  const { major, majorUnit, minor, minorUnit } = decomposeDuration(milliseconds);
  return minor === 0 || !minorUnit
    ? formatNumber(major, unitOptions(majorUnit))
    : `${formatNumber(major, unitOptions(majorUnit))} ${formatNumber(minor, unitOptions(minorUnit))}`;
}
