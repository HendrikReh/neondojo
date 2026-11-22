import { expect, it } from 'vitest';
import {
  analysisEstimateMessages,
  formatAnalysisDuration,
  analysisEstimateText,
} from './analysis-time-estimate-messages';
import {
  remainingTimeMessages,
  remainingTimeText,
} from './analysis-remaining-time-messages';
import {
  languages,
  placeholders,
  validatePluralForms,
  type Locale,
} from '../../../shared/i18n/i18n-core';

const nbsp = ' ';
it.each([
  [0.1, '<1s'],
  [999, '<1s'],
  [1000, '1s'],
  [1499, '1s'],
  [59_600, '1m'],
  [61_000, '1m 1s'],
  [3_599_600, '1h'],
  [3_660_000, '1h 1m'],
  [90_000_000, '25h'],
])('formats %s ms without zero units', (ms, expected) =>
  expect(formatAnalysisDuration('en', Number(ms))).toBe(expected),
);
it.each([0, -1, NaN, Infinity])('does not fabricate duration for %s', (ms) =>
  expect(formatAnalysisDuration('de', ms)).toBeNull(),
);
it.each([
  ['de', 999, `<1${nbsp}s`],
  ['de', 61_000, `1${nbsp}min 1${nbsp}s`],
  ['es', 40_000, `40${nbsp}s`],
  ['fr', 3_660_000, `1${nbsp}h 1${nbsp}min`],
  ['it', 120_000, `2${nbsp}min`],
  ['pl', 3_660_000, `1${nbsp}godz. 1${nbsp}min`],
  ['pl', 90_000_000, `25${nbsp}godz.`],
] as const)('writes %s duration units for %s ms', (locale, ms, expected) =>
  expect(formatAnalysisDuration(locale, ms)).toBe(expected),
);

const timingCatalogs: [string, Record<Locale, Record<string, string>>][] = [
  ['remaining time', remainingTimeMessages],
  ['analysis estimate', analysisEstimateMessages],
];
it.each(timingCatalogs)(
  'provides complete %s catalogs in all six languages',
  (_name, catalogs) => {
    const source = catalogs.en;
    expect(Object.keys(catalogs).sort()).toEqual(
      languages.map(({ id }) => id).sort(),
    );
    for (const { id } of languages) {
      const catalog = catalogs[id];
      expect(Object.keys(catalog).sort()).toEqual(Object.keys(source).sort());
      for (const key of Object.keys(source)) {
        expect(catalog[key], `${id}.${key}`).toBeTruthy();
        expect(placeholders(catalog[key] ?? ''), `${id}.${key}`).toEqual(
          placeholders(source[key] ?? ''),
        );
        expect(() => validatePluralForms(id, catalog[key] ?? '')).not.toThrow();
      }
    }
  },
);

it.each([
  ['en', 'Estimated analysis time: approx. 40s'],
  ['de', 'Geschätzte Analysezeit: ca. 40s'],
  ['es', 'Tiempo de análisis estimado: aprox. 40s'],
  ['fr', 'Durée d’analyse estimée : env. 40s'],
  ['it', 'Tempo di analisi stimato: circa 40s'],
  ['pl', 'Szacowany czas analizy: ok. 40s'],
] as const)('localizes the %s estimate', (locale, expected) =>
  expect(analysisEstimateText(locale, 'estimate', { duration: '40s' })).toBe(
    expected,
  ),
);
it.each([
  ['es', 'Estimación no disponible'],
  ['fr', 'Estimation indisponible'],
  ['it', 'Stima non disponibile'],
  ['pl', 'Szacunek niedostępny'],
] as const)('does not fall back to English in %s', (locale, expected) =>
  expect(analysisEstimateText(locale, 'unavailable')).toBe(expected),
);
it.each([
  [1, '1 ruch pionkami · 2 Ply'],
  [3, '3 ruchy pionkami · 2 Ply'],
  [5, '5 ruchów pionkami · 2 Ply'],
  [22, '22 ruchy pionkami · 2 Ply'],
])('uses Polish plural forms for %s checker moves', (moves, expected) =>
  expect(analysisEstimateText('pl', 'moves', { moves, plies: 2 })).toBe(
    expected,
  ),
);
it.each([
  ['es', 'Tiempo restante estimado: aprox. 1 min'],
  ['fr', 'Temps restant estimé : env. 1 min'],
  ['it', 'Tempo rimanente stimato: circa 1 min'],
  ['pl', 'Szacowany pozostały czas: ok. 1 min'],
] as const)('localizes the %s remaining time', (locale, expected) =>
  expect(remainingTimeText(locale, 'estimated', { duration: '1 min' })).toBe(
    expected,
  ),
);
