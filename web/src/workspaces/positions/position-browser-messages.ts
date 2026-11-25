import { defineMessages } from '../../shared/i18n/messages';
export type PositionMessageKey = keyof typeof messages.catalogs.en;

const messages = defineMessages('position-browser');
export const positionMessages = messages.catalogs;
export const positionText = messages.text;
export const usePositionText = messages.useText;
