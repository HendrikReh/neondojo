import { loadLocale } from '../src/shared/i18n/i18n-catalogs';
import { readLocale } from '../src/shared/i18n/i18n-core';

await loadLocale(readLocale());
export { default as React } from 'react';
export { createRoot } from 'react-dom/client';
