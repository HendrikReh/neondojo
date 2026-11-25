import { defineMessages } from '../../../shared/i18n/messages';
export type CardSearchMessage = keyof typeof messages.catalogs.en;

const messages = defineMessages('card-search');
export const cardSearchText = messages.text;
export const useCardSearchText = messages.useText;
