import { defineMessages } from '../../shared/i18n/messages';

const messages = defineMessages('analysis-catalog');

export type AnalysisCatalogMessage = keyof typeof messages.catalogs.en;
export const analysisCatalogText = messages.text;
export const useAnalysisCatalogText = messages.useText;
