import { useId } from 'react';
import EngineLabel from '../../shared/engine/EngineLabel';
import { useI18n } from '../../shared/i18n/i18n';
import {
  benchmarkComparisons,
  benchmarkThreadCount,
} from './benchmark-comparison';
import { benchmarkTimeDigits } from './benchmark-duration';
import { useBenchmarkText } from './benchmark-messages';
import type { BenchmarkRun } from './benchmark-api';
const labels = {
  en: ['Comparison · median of 3 runs', 'Median', 'Range', 'Initialization'],
  de: [
    'Vergleich · Median aus 3 Läufen',
    'Median',
    'Spanne',
    'Initialisierung',
  ],
  es: [
    'Comparación · mediana de 3 ejecuciones',
    'Mediana',
    'Rango',
    'Inicialización',
  ],
  fr: [
    'Comparaison · médiane de 3 mesures',
    'Médiane',
    'Étendue',
    'Initialisation',
  ],
  it: [
    'Confronto · mediana di 3 esecuzioni',
    'Mediana',
    'Intervallo',
    'Inizializzazione',
  ],
  pl: [
    'Porównanie · mediana z 3 pomiarów',
    'Mediana',
    'Zakres',
    'Inicjalizacja',
  ],
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
    <section className="benchmark-comparison" aria-labelledby={titleId}>
      <h2 id={titleId}>{t[0]}</h2>
      <div className="benchmark-table-scroll">
        <table>
          <thead>
            <tr>
              <th scope="col">{text('engineColumn')}</th>
              <th scope="col">{text('depth')}</th>
              <th scope="col" className="benchmark-number">
                {t[1]}
              </th>
              <th scope="col" className="benchmark-number">
                {t[2]}
              </th>
              <th scope="col">{text('details')}</th>
            </tr>
          </thead>
          <tbody>
            {groups.map(({ run, median, minimum, maximum }) => {
              const threads = benchmarkThreadCount(run);
              const workload = run.play_reference?.id ?? run.reference?.name;
              const protocol =
                run.play_reference?.protocol_id ?? run.protocol?.id;
              const rulesVersion =
                run.engine_id === 'bgsage'
                  ? run.engine_snapshot?.rules_version
                  : null;
              const ms = (value: number) =>
                `${formatNumber(value, benchmarkTimeDigits)} ms`;
              return (
                <tr key={run.id}>
                  <th scope="row">
                    {run.engine_name}
                    {threads != null && (
                      <small>{text('threads', { count: threads })}</small>
                    )}
                  </th>
                  <td>{formatNumber(run.plies)}-ply</td>
                  <td className="benchmark-number benchmark-median">
                    {ms(median)}
                  </td>
                  <td className="benchmark-number">
                    {formatNumber(minimum, benchmarkTimeDigits)}–{ms(maximum)}
                  </td>
                  <td>
                    <details className="benchmark-row-details">
                      <summary>{text('details')}</summary>
                      <div>
                        <strong>{run.machine.name}</strong>
                        {workload && <small>{workload}</small>}
                        <small>
                          {text('recordedEngineVersion')}:{' '}
                          <code>
                            <EngineLabel details={run.engine_version} />
                          </code>
                        </small>
                        {rulesVersion && (
                          <small>
                            {text('rulesVersion')}:{' '}
                            <code>
                              <EngineLabel details={rulesVersion} />
                            </code>
                          </small>
                        )}
                        {protocol && (
                          <small>
                            {text('protocol')}: <code>{protocol}</code>
                          </small>
                        )}
                      </div>
                    </details>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
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
      {labels[locale][3]}: {formatNumber(milliseconds, benchmarkTimeDigits)} ms
    </small>
  );
}
