type FormatNumber = (
  value: number,
  options?: Intl.NumberFormatOptions,
) => string;

const unit = (name: 'second' | 'minute' | 'hour', digits = 0) =>
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
    return formatNumber(milliseconds / 1000, unit('second', 2));
  const seconds = Math.round(milliseconds / 1000);
  const [large, small, largeUnit, smallUnit] =
    seconds < 3600
      ? ([Math.floor(seconds / 60), seconds % 60, 'minute', 'second'] as const)
      : ([
          Math.floor(seconds / 3600),
          Math.round((seconds % 3600) / 60),
          'hour',
          'minute',
        ] as const);
  if (smallUnit === 'minute' && small === 60)
    return formatNumber(large + 1, unit('hour'));
  return small === 0
    ? formatNumber(large, unit(largeUnit))
    : `${formatNumber(large, unit(largeUnit))} ${formatNumber(small, unit(smallUnit))}`;
}
