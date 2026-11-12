import { useI18n } from './i18n';
import { benchmarkComparisons } from './benchmark-comparison';
import type { BenchmarkRun } from './benchmark-api';
const labels = {
  en: ['Three comparable runs', 'Median', 'Range', 'Initialization'],
  de: ['Drei vergleichbare Läufe', 'Median', 'Spanne', 'Initialisierung'],
  es: ['Tres ejecuciones comparables', 'Mediana', 'Rango', 'Inicialización'],
  fr: ['Trois mesures comparables', 'Médiane', 'Étendue', 'Initialisation'],
  it: [
    'Tre esecuzioni confrontabili',
    'Mediana',
    'Intervallo',
    'Inizializzazione',
  ],
  pl: ['Trzy porównywalne pomiary', 'Mediana', 'Zakres', 'Inicjalizacja'],
};
export default function BenchmarkComparison({
  runs,
}: {
  runs: BenchmarkRun[];
}) {
  const { locale, formatNumber } = useI18n();
  const t = labels[locale];
  const groups = benchmarkComparisons(runs);
  if (!groups.length) return null;
  return (
    <section className="benchmark-help">
      <h2>{t[0]}</h2>
      {groups.map(({ run, median, minimum, maximum }) => (
        <p key={run.id}>
          {run.engine_name} · {run.plies}-ply ·{' '}
          {run.play_reference?.id ?? run.reference?.name}: {t[1]}{' '}
          {formatNumber(median, { maximumFractionDigits: 2 })} ms · {t[2]}{' '}
          {formatNumber(minimum, { maximumFractionDigits: 2 })}–
          {formatNumber(maximum, { maximumFractionDigits: 2 })} ms
        </p>
      ))}
    </section>
  );
}

export function BenchmarkInitialization({
  milliseconds,
}: {
  milliseconds?: number | null;
}) {
  const { locale, formatNumber } = useI18n();
  return milliseconds == null ? null : (
    <small>
      {labels[locale][3]}:{' '}
      {formatNumber(milliseconds, { maximumFractionDigits: 2 })} ms
    </small>
  );
}
