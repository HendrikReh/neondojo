import type { Locale } from '../../../shared/i18n/i18n';
import {
  formatMessage,
  numberFormat,
  type TranslationParams,
} from '../../../shared/i18n/i18n-core';

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
export const analysisEstimateMessages = {
  en,
  de: {
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
  },
  es: {
    label: 'Estimación del tiempo de análisis',
    estimate: 'Tiempo de análisis estimado: aprox. {duration}',
    loading: 'Estimando la duración …',
    unavailable: 'Estimación no disponible',
    no_matching_benchmark:
      'Ningún benchmark completado coincide con esta instalación del motor, este equipo y esta profundidad.',
    engine_unavailable: 'El motor de análisis configurado no está disponible.',
    no_checker_moves:
      'No hay jugadas de fichas en este ámbito; puede que aún haga falta analizar el cubo.',
    network: 'No se pudo cargar la estimación.',
    configuration: 'Esperando a que se apliquen los ajustes de análisis.',
    game: 'Selecciona una partida para estimar su duración.',
    moves:
      '{moves, plural, one {# jugada de fichas} other {# jugadas de fichas}} · {plies} Ply',
    basis: 'Benchmark {date} · {engine}',
    caveat:
      'Sin contar la espera; la duración varía según la complejidad de las posiciones.',
  },
  fr: {
    label: 'Estimation de la durée d’analyse',
    estimate: 'Durée d’analyse estimée : env. {duration}',
    loading: 'Estimation de la durée …',
    unavailable: 'Estimation indisponible',
    no_matching_benchmark:
      'Aucun benchmark terminé ne correspond à cette installation du moteur, à cette machine et à cette profondeur.',
    engine_unavailable: 'Le moteur d’analyse configuré est indisponible.',
    no_checker_moves:
      'Aucun coup de pions dans ce périmètre ; une analyse du cube peut néanmoins être nécessaire.',
    network: 'L’estimation n’a pas pu être chargée.',
    configuration: 'En attente de l’application des paramètres d’analyse.',
    game: 'Sélectionnez une partie pour estimer sa durée.',
    moves:
      '{moves, plural, one {# coup de pions} other {# coups de pions}} · {plies} Ply',
    basis: 'Benchmark {date} · {engine}',
    caveat:
      'Attente exclue ; la durée varie selon la complexité des positions.',
  },
  it: {
    label: 'Stima del tempo di analisi',
    estimate: 'Tempo di analisi stimato: circa {duration}',
    loading: 'Stima della durata in corso …',
    unavailable: 'Stima non disponibile',
    no_matching_benchmark:
      'Nessun benchmark completato corrisponde a questa installazione del motore, a questo computer e a questa profondità.',
    engine_unavailable: 'Il motore di analisi configurato non è disponibile.',
    no_checker_moves:
      'Nessuna mossa di pedine in questo ambito; potrebbe comunque servire l’analisi del cubo.',
    network: 'Impossibile caricare la stima.',
    configuration:
      'In attesa che vengano applicate le impostazioni di analisi.',
    game: 'Seleziona una partita per stimarne la durata.',
    moves:
      '{moves, plural, one {# mossa di pedine} other {# mosse di pedine}} · {plies} Ply',
    basis: 'Benchmark {date} · {engine}',
    caveat:
      'Attesa esclusa; la durata varia con la complessità delle posizioni.',
  },
  pl: {
    label: 'Szacowany czas analizy',
    estimate: 'Szacowany czas analizy: ok. {duration}',
    loading: 'Szacowanie czasu trwania …',
    unavailable: 'Szacunek niedostępny',
    no_matching_benchmark:
      'Żaden ukończony benchmark nie pasuje do tej instalacji silnika, tego komputera i tej głębokości.',
    engine_unavailable: 'Skonfigurowany silnik analizy jest niedostępny.',
    no_checker_moves:
      'Brak ruchów pionkami w tym zakresie; analiza kostki może być jednak potrzebna.',
    network: 'Nie udało się wczytać szacunku.',
    configuration: 'Oczekiwanie na zastosowanie ustawień analizy.',
    game: 'Wybierz partię, aby oszacować czas jej analizy.',
    moves:
      '{moves, plural, one {# ruch pionkami} few {# ruchy pionkami} many {# ruchów pionkami}} · {plies} Ply',
    basis: 'Benchmark {date} · {engine}',
    caveat: 'Bez czasu oczekiwania; czas trwania zależy od złożoności pozycji.',
  },
} satisfies Record<Locale, Record<Key, string>>;
export function analysisEstimateText(
  locale: Locale,
  key: Key,
  values?: TranslationParams,
) {
  return formatMessage(locale, analysisEstimateMessages[locale][key], values);
}

type DurationUnit = 'seconds' | 'minutes' | 'hours';
/**
 * Compact duration units. `{n}` is the amount in the language's number
 * format; a no-break space keeps it on the same line as its unit.
 */
export const durationUnits = {
  en: { underSecond: '<1s', seconds: '{n}s', minutes: '{n}m', hours: '{n}h' },
  de: {
    underSecond: '<1 s',
    seconds: '{n} s',
    minutes: '{n} min',
    hours: '{n} h',
  },
  es: {
    underSecond: '<1 s',
    seconds: '{n} s',
    minutes: '{n} min',
    hours: '{n} h',
  },
  fr: {
    underSecond: '<1 s',
    seconds: '{n} s',
    minutes: '{n} min',
    hours: '{n} h',
  },
  it: {
    underSecond: '<1 s',
    seconds: '{n} s',
    minutes: '{n} min',
    hours: '{n} h',
  },
  pl: {
    underSecond: '<1 s',
    seconds: '{n} s',
    minutes: '{n} min',
    hours: '{n} godz.',
  },
} satisfies Record<Locale, Record<'underSecond' | DurationUnit, string>>;

/** Rounded remaining or estimated time, omitting zero sub-units. */
export function formatAnalysisDuration(
  locale: Locale,
  ms: number,
): string | null {
  if (!Number.isFinite(ms) || ms <= 0) return null;
  const units = durationUnits[locale];
  if (ms < 1000) return units.underSecond;
  const amount = (unit: DurationUnit, n: number) =>
    units[unit].replace('{n}', numberFormat(locale).format(n));
  const join = (major: string, unit: DurationUnit, remainder: number) =>
    remainder ? `${major} ${amount(unit, remainder)}` : major;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return amount('seconds', seconds);
  if (seconds < 3600)
    return join(
      amount('minutes', Math.floor(seconds / 60)),
      'seconds',
      seconds % 60,
    );
  const minutes = Math.round(ms / 60_000);
  return join(
    amount('hours', Math.floor(minutes / 60)),
    'minutes',
    minutes % 60,
  );
}
