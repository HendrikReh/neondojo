import type { Locale } from '../../shared/i18n/i18n';
import { defineMessages } from '../../shared/i18n/messages';

const messages = defineMessages('engine-match');
export type EngineMatchLabel = keyof typeof messages.catalogs.en;
export const engineMatchMessages = messages.catalogs;
export const engineMatchText = (locale: Locale, key: EngineMatchLabel) =>
  messages.text(locale, key);
export const useEngineMatchText = messages.useText;
