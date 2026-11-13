import { useId } from 'react';
import { useI18n } from './i18n';
import {
  benchmarkComparisons,
  benchmarkThreadCount,
} from './benchmark-comparison';
import { useBenchmarkText } from './benchmark-messages';
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
  const text = useBenchmarkText();
  const groups = benchmarkComparisons(runs);
  const titleId = useId();
  if (!groups.length) return null;
  return (
    <section
      className="benchmark-help benchmark-comparison"
      aria-labelledby={titleId}
    >
      <h2 id={titleId}>{t[0]}</h2>
      {groups.map(({ run, median, minimum, maximum }) => {
        const threads = benchmarkThreadCount(run);
        const workload = run.play_reference?.id ?? run.reference?.name;
        const protocol = run.play_reference?.protocol_id ?? run.protocol?.id;
        const rulesVersion =
          run.engine_id === 'bgsage'
            ? run.engine_snapshot?.rules_version
            : null;
        return (
          <details key={run.id}>
            <summary>
              <strong>
                {run.engine_name} · {formatNumber(run.plies)}-ply
              </strong>
              {threads != null &&
                ` · ${text('threads', { count: formatNumber(threads) })}`}
              {' · '}
              {run.machine.name}
              {workload && ` · ${workload}`}
              {' · '}
              {t[1]} {formatNumber(median, { maximumFractionDigits: 2 })} ms ·{' '}
              {t[2]} {formatNumber(minimum, { maximumFractionDigits: 2 })}–
              {formatNumber(maximum, { maximumFractionDigits: 2 })} ms
            </summary>
            <dl>
              <div>
                <dt>{text('recordedEngineVersion')}</dt>
                <dd>
                  <code>{run.engine_version}</code>
                </dd>
              </div>
              {rulesVersion && (
                <div>
                  <dt>{text('rulesVersion')}</dt>
                  <dd>
                    <code>{rulesVersion}</code>
                  </dd>
                </div>
              )}
              {protocol && (
                <div>
                  <dt>{text('protocol')}</dt>
                  <dd>
                    <code>{protocol}</code>
                  </dd>
                </div>
              )}
            </dl>
          </details>
        );
      })}
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
