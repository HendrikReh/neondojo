import type { Locale } from '../../../shared/i18n/i18n';
import { defineMessages } from '../../../shared/i18n/messages';
import { decomposeDuration, type DurationUnit } from '../../../shared/format/duration';

const messages = defineMessages('analysis-time-estimate');

export const analysisEstimateMessages = messages.catalogs;
export const analysisEstimateText = messages.text;

const unitKey: Record<DurationUnit, 'seconds' | 'minutes' | 'hours'> = {
  second: 'seconds',
  minute: 'minutes',
  hour: 'hours',
};

/**
 * Compact duration units. `{n}` is the amount in the language's number
 * format; a no-break space keeps it on the same line as its unit.
 */
const durationMessages = defineMessages('analysis-duration-units');
export const durationUnits = durationMessages.catalogs;

/** Rounded remaining or estimated time, omitting zero sub-units. */
export function formatAnalysisDuration(
  locale: Locale,
  ms: number,
): string | null {
  if (!Number.isFinite(ms) || ms <= 0) return null;
  if (ms < 1000) return durationMessages.text(locale, 'underSecond');
  const { major, majorUnit, minor, minorUnit } = decomposeDuration(ms);
  const first = durationMessages.text(locale, unitKey[majorUnit], { n: major });
  if (!minor || !minorUnit) return first;
  return `${first} ${durationMessages.text(locale, unitKey[minorUnit], { n: minor })}`;
}
