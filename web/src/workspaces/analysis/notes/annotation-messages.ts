import { useI18n } from '../../../shared/i18n/i18n';
import { defineMessages } from '../../../shared/i18n/messages';

export type AnnotationMessageKey = keyof typeof messages.catalogs.en;

const messages = defineMessages('annotation');

export const annotationMessages = messages.catalogs;
export const annotationText = messages.text;

export function useAnnotationI18n() {
  const i18n = useI18n();
  const t = messages.useText();
  return { ...i18n, t };
}
