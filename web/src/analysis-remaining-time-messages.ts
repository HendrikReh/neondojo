import type { Locale } from './shared/i18n/i18n';
import { interpolate, type TranslationParams } from './shared/i18n/i18n-core';
const en = {
  estimated: 'Estimated time remaining: approx. {duration}',
  loading: 'Calculating time remaining …',
  queued: 'Waiting for analysis to start',
  waiting: 'Waiting for engine availability',
  reestimating: 'Recalculating time remaining …',
  unavailable: 'Remaining time is not yet available',
  finishing: 'Saving analysis results …',
  benchmark: 'Based on the saved benchmark',
  observed: 'Based on the measured analysis speed',
};
type Key = keyof typeof en;
const de: Record<Key, string> = {
  estimated: 'Geschätzte Restzeit: ca. {duration}',
  loading: 'Restzeit wird ermittelt …',
  queued: 'Wartet auf Analysebeginn',
  waiting: 'Wartet auf Engineverfügbarkeit',
  reestimating: 'Restzeit wird neu geschätzt …',
  unavailable: 'Restzeit ist noch nicht verfügbar',
  finishing: 'Analyseergebnisse werden gespeichert …',
  benchmark: 'Auf Basis des gespeicherten Benchmarks',
  observed: 'Auf Basis des gemessenen Analysetempos',
};
export function remainingTimeText(
  locale: Locale,
  key: Key,
  values?: TranslationParams,
) {
  return interpolate((locale === 'de' ? de : en)[key], values);
}
