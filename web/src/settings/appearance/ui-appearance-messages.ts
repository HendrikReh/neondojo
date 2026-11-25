import { defineMessages } from '../../shared/i18n/messages';
export type UiAppearanceMessageKey = keyof typeof messages.catalogs.en;
const messages = defineMessages('ui-appearance');
export const uiAppearanceMessages = messages.catalogs;
export const uiAppearanceText = messages.text;
export const useUiAppearanceText = messages.useText;
