import type { Locale } from '../../../shared/i18n/i18n-core';
import { defineMessages } from '../../../shared/i18n/messages';

const messages = defineMessages('analysis-source');

export const sourceText = messages.text;
export const useSourceText = messages.useText;

export const openingBookProviders = [
  'automatic',
  'backgammondb',
  'xg',
  'off',
] as const;

export function openingBookProviderLabel(
  locale: Locale,
  provider: (typeof openingBookProviders)[number],
) {
  switch (provider) {
    case 'backgammondb':
      return 'BackgammonDB';
    case 'xg':
      return 'eXtreme Gammon (XG)';
    case 'automatic':
    case 'off':
      return sourceText(locale, provider);
  }
}
