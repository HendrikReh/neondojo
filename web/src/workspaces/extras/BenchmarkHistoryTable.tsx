import { useMemo, useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import EngineLabel from '../../shared/engine/EngineLabel';
import LibrarySortHeading from '../../shared/ui/LibrarySortHeading';
import { formatPly } from '../../shared/format/ply';
import { useI18n } from '../../shared/i18n/i18n';
import { dateTimeFormat } from '../../shared/i18n/i18n-core';
import { BenchmarkInitialization } from './BenchmarkComparison';
import {
  benchmarkTimeDigits,
  formatBenchmarkDuration,
} from './benchmark-duration';
import {
  DEFAULT_BENCHMARK_HISTORY_ORDER,
  nextBenchmarkHistoryOrder,
  sortBenchmarkRuns,
  type BenchmarkHistorySort,
} from './benchmark-history-sort';
import type { BenchmarkRun } from './benchmark-api';
import { useBenchmarkText } from './benchmark-messages';
import { benchmarkProgress, benchmarkStatusLabel } from './useBenchmarkRuns';

export default function BenchmarkHistoryTable({
  runs,
  play,
  loading,
  deleting,
  onDelete,
}: {
  runs: BenchmarkRun[];
  play: boolean;
  loading: boolean;
  deleting: string | undefined;
  onDelete: (id: string) => Promise<boolean>;
}) {
  const t = useBenchmarkText();
  const { locale, formatNumber } = useI18n();
  const [confirmingDelete, setConfirmingDelete] = useState<string>();
  const [historyOrder, setHistoryOrder] = useState(
    DEFAULT_BENCHMARK_HISTORY_ORDER,
  );
  const historyRuns = useMemo(
    () => sortBenchmarkRuns(runs, historyOrder, play, locale),
    [runs, historyOrder, play, locale],
  );
  const dateFormatter = dateTimeFormat(locale);
  const time = (run: BenchmarkRun, value: number | null | undefined) =>
    run.status === 'completed' && typeof value === 'number'
      ? `${formatNumber(value, benchmarkTimeDigits)} ms`
      : '—';

  const sortHeading = (
    sort: BenchmarkHistorySort,
    label: string,
    className?: string,
  ) => (
    <LibrarySortHeading<BenchmarkHistorySort>
      fields={[{ key: sort, label }]}
      sort={historyOrder.sort}
      direction={historyOrder.direction}
      className={className}
      onSort={(next) =>
        setHistoryOrder((current) => nextBenchmarkHistoryOrder(current, next))
      }
    />
  );

  async function confirmRemove(id: string) {
    if (await onDelete(id)) setConfirmingDelete(undefined);
  }

  return (
    <section className="benchmark-history">
      <h2>{t('history')}</h2>
      {loading ? (
        <p role="status">{t('loading')}</p>
      ) : runs.length === 0 ? (
        <p>{t('empty')}</p>
      ) : (
        <div className="benchmark-table-scroll">
          <table>
            <thead>
              <tr>
                {sortHeading('date', t('date'))}
                {sortHeading('engine', t('engineColumn'))}
                {sortHeading('plies', t('depth'))}
                {!play && sortHeading('total', t('total'), 'benchmark-number')}
                {sortHeading(
                  'average',
                  t(play ? 'playAverage' : 'average'),
                  'benchmark-number',
                )}
                {sortHeading('status', t('status'))}
                <th scope="col">{t('details')}</th>
                <th scope="col">
                  <span className="sr-only">{t('actions')}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {historyRuns.map((run) => (
                <tr key={run.id}>
                  <td>
                    <time dateTime={run.created_at}>
                      {dateFormatter.format(new Date(run.created_at))}
                    </time>
                  </td>
                  <td>{run.engine_name}</td>
                  <td>{formatPly(run.plies, formatNumber)}</td>
                  {!play && (
                    <td
                      className="benchmark-number"
                      data-benchmark-time
                      title={
                        run.status === 'completed' &&
                        typeof run.total_elapsed_ms === 'number' &&
                        run.total_elapsed_ms >= 1000
                          ? time(run, run.total_elapsed_ms)
                          : undefined
                      }
                    >
                      {run.status === 'completed' &&
                      typeof run.total_elapsed_ms === 'number'
                        ? formatBenchmarkDuration(
                            run.total_elapsed_ms,
                            formatNumber,
                          )
                        : '—'}
                    </td>
                  )}
                  <td
                    className={
                      play
                        ? 'benchmark-number benchmark-latency'
                        : 'benchmark-number'
                    }
                    data-benchmark-time
                  >
                    {time(
                      run,
                      play ? run.average_turn_ms : run.average_move_ms,
                    )}
                  </td>
                  <td>
                    <span
                      className={`benchmark-status benchmark-status-${run.status}`}
                    >
                      {benchmarkStatusLabel(run.status, t)}
                    </span>
                    {run.status !== 'completed' && (
                      <small>{benchmarkProgress(run, t, formatNumber)}</small>
                    )}
                    {run.error && (
                      <small className="benchmark-run-error">{run.error}</small>
                    )}
                  </td>
                  <td>
                    <BenchmarkRunDetails run={run} />
                  </td>
                  <td className="benchmark-row-actions">
                    {run.status !== 'queued' &&
                      run.status !== 'running' &&
                      (confirmingDelete === run.id ? (
                        <div
                          className="benchmark-delete-confirm"
                          role="group"
                          aria-label={t('deleteQuestion')}
                          onKeyDown={(event) => {
                            if (event.key === 'Escape' && !deleting) {
                              event.stopPropagation();
                              setConfirmingDelete(undefined);
                            }
                          }}
                        >
                          <span>{t('deleteQuestion')}</span>
                          <ActionButton
                            icon="delete"
                            className="danger"
                            type="button"
                            label={t(
                              deleting === run.id
                                ? 'deleting'
                                : 'confirmDelete',
                            )}
                            autoFocus
                            disabled={Boolean(deleting)}
                            onClick={() => void confirmRemove(run.id)}
                          />
                          <ActionButton
                            icon="cancel"
                            type="button"
                            label={t('keepRun')}
                            disabled={Boolean(deleting)}
                            onClick={() => setConfirmingDelete(undefined)}
                          />
                        </div>
                      ) : (
                        <ActionButton
                          icon="delete"
                          type="button"
                          label={t('deleteRun')}
                          compact
                          disabled={Boolean(deleting)}
                          onClick={() => setConfirmingDelete(run.id)}
                        />
                      ))}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function BenchmarkRunDetails({ run }: { run: BenchmarkRun }) {
  const t = useBenchmarkText();
  const { formatNumber } = useI18n();
  const reference = run.play_reference ?? run.reference;

  return (
    <details className="benchmark-row-details">
      <summary>{t('details')}</summary>
      <div>
        <strong>{run.machine.name}</strong>
        <small>
          {run.machine.os} · {run.machine.architecture}
        </small>
        <small>
          {[
            run.machine.cpu_model,
            run.machine.logical_cpus == null
              ? undefined
              : t('cpus', { count: run.machine.logical_cpus }),
            run.machine.memory_bytes == null
              ? undefined
              : `${formatNumber(run.machine.memory_bytes / 2 ** 30, { maximumFractionDigits: 1 })} GiB`,
          ]
            .filter(Boolean)
            .join(' · ')}
        </small>
        <small>
          <EngineLabel details={run.engine_version} />
        </small>
        <BenchmarkInitialization milliseconds={run.startup_elapsed_ms} />
        <small>
          <code>{run.engine_path}</code>
        </small>
        {reference && (
          <>
            {run.reference && (
              <small>
                {run.reference.name} ·{' '}
                {t('referenceVersion', {
                  version: formatNumber(run.reference.version),
                })}
              </small>
            )}
            <small>
              <code>{reference.id}</code>
            </small>
            <small>
              <code>{run.play_reference?.protocol_id ?? run.protocol?.id}</code>
            </small>
            <small>
              SHA-256 <code>{reference.sha256}</code>
            </small>
          </>
        )}
      </div>
    </details>
  );
}
