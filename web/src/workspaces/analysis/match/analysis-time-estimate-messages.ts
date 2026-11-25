import type { Locale } from '../../../shared/i18n/i18n';
import { defineMessages } from '../../../shared/i18n/messages';
const messages = defineMessages('analysis-time-estimate');

export const analysisEstimateMessages = messages.catalogs;
export const analysisEstimateText = messages.text;

type DurationUnit = 'seconds' | 'minutes' | 'hours';
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
  const amount = (unit: DurationUnit, n: number) =>
    durationMessages.text(locale, unit, { n });
  const join = (major: string, unit: DurationUnit, remainder: number) =>
    remainder ? `${major} ${amount(unit, remainder)}` : major;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return amount('seconds', seconds);
  if (seconds < 3600)
    return join(
      amount('minutes', Math.floor(seconds / 60)),
      'seconds',
      seconds % 60,
    );
  const minutes = Math.round(ms / 60_000);
  return join(
    amount('hours', Math.floor(minutes / 60)),
    'minutes',
    minutes % 60,
  );
}
