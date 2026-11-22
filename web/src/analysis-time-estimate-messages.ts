import type { Locale } from './shared/i18n/i18n';
import { formatMessage, type TranslationParams } from './shared/i18n/i18n-core';

const en = {
  label: 'Analysis time estimate',
  estimate: 'Estimated analysis time: approx. {duration}',
  loading: 'Estimating duration …',
  unavailable: 'Estimate unavailable',
  no_matching_benchmark:
    'No completed benchmark matches this engine installation, machine and depth.',
  engine_unavailable: 'The configured analysis engine is unavailable.',
  no_checker_moves:
    'No checker moves in this scope; cube analysis may still be needed.',
  network: 'The estimate could not be loaded.',
  configuration: 'Waiting for the analysis settings to be applied.',
  game: 'Select a game to estimate its duration.',
  moves:
    '{moves, plural, one {# checker move} other {# checker moves}} · {plies} Ply',
  basis: 'Benchmark {date} · {engine}',
  caveat: 'Queueing excluded; time varies with position complexity.',
};
type Key = keyof typeof en;
const de: Record<Key, string> = {
  label: 'Geschätzte Analysezeit',
  estimate: 'Geschätzte Analysezeit: ca. {duration}',
  loading: 'Dauer wird geschätzt …',
  unavailable: 'Schätzung nicht verfügbar',
  no_matching_benchmark:
    'Kein abgeschlossener Benchmark passt zu dieser Engine-Installation, diesem Rechner und dieser Analysetiefe.',
  engine_unavailable: 'Die konfigurierte Analyse-Engine ist nicht verfügbar.',
  no_checker_moves:
    'Keine Checkerzüge in diesem Umfang; Cube-Analyse kann dennoch nötig sein.',
  network: 'Die Schätzung konnte nicht geladen werden.',
  configuration: 'Wartet, bis die Analyseeinstellungen angewendet sind.',
  game: 'Wähle ein Spiel für die Laufzeitschätzung.',
  moves:
    '{moves, plural, one {# Checkerzug} other {# Checkerzüge}} · {plies} Ply',
  basis: 'Benchmark {date} · {engine}',
  caveat:
    'Ohne Warteschlange; die Dauer hängt von der Komplexität der Stellungen ab.',
};
export function analysisEstimateText(
  locale: Locale,
  key: Key,
  values?: TranslationParams,
) {
  return formatMessage(locale, (locale === 'de' ? de : en)[key], values);
}

export function formatAnalysisDuration(ms: number): string | null {
  if (!Number.isFinite(ms) || ms <= 0) return null;
  if (ms < 1000) return '<1s';
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) {
    const remainder = seconds % 60;
    return `${Math.floor(seconds / 60)}m${remainder ? ` ${remainder}s` : ''}`;
  }
  const minutes = Math.round(ms / 60_000);
  const remainder = minutes % 60;
  return `${Math.floor(minutes / 60)}h${remainder ? ` ${remainder}m` : ''}`;
}
