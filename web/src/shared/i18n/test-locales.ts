// Test processes exercise translations in all languages, sometimes during test collection.
import { languages } from './i18n-core';
import { loadLocale } from './i18n-catalogs';

await Promise.all(languages.map(({ id }) => loadLocale(id)));
