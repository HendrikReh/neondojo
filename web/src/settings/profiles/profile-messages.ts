import type { Locale } from '../../shared/i18n/i18n-core';
import { defineMessages } from '../../shared/i18n/messages';
export type ProfileMessageKey = keyof typeof messages.catalogs.en;
const messages = defineMessages('profile');
export const profileCatalogs = messages.catalogs;
export const profileText = messages.text;
export const useProfileText = messages.useText;
export function profileFieldKey(
  locale: Locale,
  path: string,
): ProfileMessageKey | undefined {
  const leaf = path.split('.').at(-1)!;
  return Object.hasOwn(messages.catalogs[locale], leaf)
    ? (leaf as ProfileMessageKey)
    : undefined;
}
