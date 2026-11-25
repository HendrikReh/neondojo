import { type TranslationKey } from '../i18n/i18n';
import { defineMessages } from '../i18n/messages';

export const errorThresholdLabels = {
  questionable: 'severity.doubtful',
  error: 'severity.bad',
  blunder: 'severity.blunder',
  large_blunder: 'severity.very_bad',
} as const satisfies Record<
  keyof import('../../settings/configuration/configuration-model').ErrorThresholds,
  TranslationKey
>;

export const severityLabels: Record<string, TranslationKey> = {
  none: 'severity.none',
  doubtful: errorThresholdLabels.questionable,
  bad: errorThresholdLabels.error,
  blunder: errorThresholdLabels.blunder,
  very_bad: errorThresholdLabels.large_blunder,
};

const messages = defineMessages('review-statistics');
export const statisticsMessages = messages.catalogs;
export const statisticsText = messages.text;
export const useStatisticsText = messages.useText;
