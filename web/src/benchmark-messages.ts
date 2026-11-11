import { useI18n, type Locale } from './i18n';
import { interpolate, type TranslationParams } from './i18n-core';
const en = {
  globalBusy:
    'Benchmark running or waiting. Other engine actions wait until measurement finishes.',
  extras: 'Extras',
  benchmark: 'Benchmark',
  navigation: 'Extra tools',
  title: 'Engine benchmark',
  description: 'Compare full-match analysis times on this backend machine.',
  reference: 'Fixed reference',
  referenceFacts: '{points} points · {games} games · {moves} checker moves',
  referenceVersion: 'Reference version {version}',
  protocol: 'Protocol',
  timeout: 'Timeout: {seconds} seconds per game',
  noTimeout: 'No time limit',
  engine: 'Benchmark engine',
  plies: 'Analysis depth',
  start: 'Start benchmark',
  starting: 'Starting …',
  noEngine:
    'No available analysis engine. Check the GNUbg installation in Settings.',
  timing:
    'Total time includes engine startup, loading, checker and cube analysis, and result preparation. Queueing and saving are excluded. Average = total time / analyzed checker moves; cube analysis and startup are included. This is a match average, not an individual move latency.',
  serial:
    'Selected depths run sequentially. Runs continue when you leave this view.',
  engineBusy: 'Other engine work waits until the benchmark queue finishes.',
  active: 'Current runs',
  history: 'History',
  empty: 'No benchmark runs yet.',
  loading: 'Loading …',
  retry: 'Try again',
  queued: 'Queued',
  running: 'Running',
  completed: 'Completed',
  failed: 'Failed',
  cancelled: 'Cancelled',
  waiting:
    'Waiting for engine availability; other engine work must finish first.',
  progress:
    '{completed} / {total} games · {moves} / {totalMoves} checker moves',
  cancel: 'Cancel run',
  cancelling: 'Cancelling …',
  date: 'Date',
  machine: 'Backend machine / hardware',
  engineVersion: 'Engine / version',
  depth: 'Ply',
  total: 'Total analysis time',
  average: 'Ø analysis time / move',
  status: 'Status',
  cpus: '{count} logical CPUs',
  open: 'Open benchmark',
  draftHelp:
    'Opening the benchmark closes Settings and discards unsaved changes.',
};
type Key = keyof typeof en;
const de: Record<Key, string> = {
  globalBusy:
    'Benchmark läuft oder wartet. Andere Engine-Aktionen warten, bis die Messung abgeschlossen ist.',
  extras: 'Extras',
  benchmark: 'Benchmark',
  navigation: 'Zusatzwerkzeuge',
  title: 'Engine-Benchmark',
  description:
    'Analysezeiten eines vollständigen Matches auf diesem Backend-Rechner vergleichen.',
  reference: 'Feste Referenz',
  referenceFacts: '{points} Punkte · {games} Spiele · {moves} Checkerzüge',
  referenceVersion: 'Referenzversion {version}',
  protocol: 'Protokoll',
  timeout: 'Zeitlimit: {seconds} Sekunden pro Spiel',
  noTimeout: 'Ohne Zeitlimit',
  engine: 'Benchmark-Engine',
  plies: 'Analysetiefe',
  start: 'Benchmark starten',
  starting: 'Wird gestartet …',
  noEngine:
    'Keine Analyse-Engine verfügbar. Prüfe die GNUbg-Installation in den Einstellungen.',
  timing:
    'Die Gesamtzeit umfasst Engine-Starts, Laden, Checker- und Cube-Analyse sowie Ergebnisaufbereitung. Wartezeit und Speichern sind ausgeschlossen. Durchschnitt = Gesamtzeit / analysierte Checkerzüge; Cube-Analyse und Prozessstarts sind enthalten. Dies ist ein Matchdurchschnitt, keine einzeln gemessene Zuglatenz.',
  serial:
    'Gewählte Tiefen laufen nacheinander. Läufe werden beim Verlassen dieser Ansicht fortgesetzt.',
  engineBusy:
    'Andere Engine-Arbeit wartet, bis die Benchmark-Warteschlange abgeschlossen ist.',
  active: 'Aktuelle Läufe',
  history: 'Historie',
  empty: 'Noch keine Benchmarkläufe.',
  loading: 'Wird geladen …',
  retry: 'Erneut versuchen',
  queued: 'Wartend',
  running: 'Läuft',
  completed: 'Abgeschlossen',
  failed: 'Fehlgeschlagen',
  cancelled: 'Abgebrochen',
  waiting:
    'Wartet auf die Engine; andere Engine-Arbeit muss zunächst abgeschlossen werden.',
  progress: '{completed} / {total} Spiele · {moves} / {totalMoves} Checkerzüge',
  cancel: 'Lauf abbrechen',
  cancelling: 'Wird abgebrochen …',
  date: 'Datum',
  machine: 'Backend-Rechner / Hardware',
  engineVersion: 'Engine / Version',
  depth: 'Ply',
  total: 'Gesamtanalysezeit',
  average: 'Ø Analysezeit / Zug',
  status: 'Status',
  cpus: '{count} logische CPUs',
  open: 'Benchmark öffnen',
  draftHelp:
    'Beim Öffnen des Benchmarks werden die Einstellungen geschlossen und ungespeicherte Änderungen verworfen.',
};
export const benchmarkText = (
  locale: Locale,
  key: Key,
  values?: TranslationParams,
) => interpolate((locale === 'de' ? de : en)[key], values);
export function useBenchmarkText() {
  const { locale } = useI18n();
  return (key: Key, values?: TranslationParams) =>
    benchmarkText(locale, key, values);
}
