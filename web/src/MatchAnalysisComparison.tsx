import { useEffect, useState } from 'react';
import EngineLabel from './EngineLabel';
import { request, type Job } from './api';
import { useI18n, type TranslationKey } from './i18n';
import { nativePerformanceRating } from './match-statistics-api';
import { useStatisticsText } from './review-statistics-messages';
import type { components } from './generated-api';

type ComparisonRun = components['schemas']['AnalysisComparisonRun'];

const engines = [
  { id: 'gnubg', name: 'GNUbg' },
  { id: 'bgsage', name: 'Open Sage' },
] as const;
const statusLabels: Record<Job['status'], TranslationKey> = {
  completed: 'matchAnalysis.completed',
  failed: 'matchAnalysis.failed',
  queued: 'matchAnalysis.queued',
  running: 'matchAnalysis.engineRunning',
};

function completeSummary(job: ComparisonRun | undefined, gameNo?: number) {
  const fromMatch = gameNo !== undefined && job?.game_no == null;
  const summary = fromMatch ? job?.game_summary : job?.result;
  const totalGames = fromMatch ? 1 : job?.total_games;
  if (
    job?.status !== 'completed' ||
    !summary ||
    !('players' in summary) ||
    job.completed_games !== job.total_games ||
    summary.total_games !== totalGames ||
    summary.completed_games !== totalGames ||
    summary.plies !== job.plies
  )
    return undefined;
  return summary;
}

export function MatchAnalysisComparisonResults({
  jobs,
  names,
  gameNo,
}: {
  jobs: ComparisonRun[];
  names: string[];
  gameNo?: number;
}) {
  const { formatNumber, locale, t } = useI18n();
  const text = useStatisticsText();
  const [selection, setSelection] = useState<Record<string, string>>({});
  const date = new Intl.DateTimeFormat(locale, {
    dateStyle: 'short',
    timeStyle: 'short',
  });
  const columns = engines.map((engine) => {
    const options = jobs.filter(
      (job) => (job.engine_snapshot?.engine_id ?? 'gnubg') === engine.id,
    );
    const job =
      options.find((option) => option.id === selection[engine.id]) ??
      options.find(
        (option) => option.plies === 3 && completeSummary(option, gameNo),
      ) ??
      options.find((option) => completeSummary(option, gameNo)) ??
      options[0];
    return { engine, options, job, summary: completeSummary(job, gameNo) };
  });
  const decimal = (value: number) =>
    formatNumber(value, { minimumFractionDigits: 3, maximumFractionDigits: 3 });
  const unavailable = <span title={text('unavailable')}>—</span>;

  return (
    <>
      <div className="statistics-comparison-scroll">
        <table className="statistics-games-table statistics-comparison-table">
          <thead>
            <tr>
              <th scope="col">{text('compareMetric')}</th>
              {columns.map(({ engine, options, job, summary }) => (
                <th key={engine.id} scope="col">
                  <div>{engine.name}</div>
                  {job ? (
                    <>
                      <select
                        aria-label={text('compareRun', { engine: engine.name })}
                        value={job.id}
                        onChange={(event) => {
                          const id = event.target.value;
                          setSelection((current) => ({
                            ...current,
                            [engine.id]: id,
                          }));
                        }}
                      >
                        {options.map((option) => (
                          <option key={option.id} value={option.id}>
                            {text('compareDepth', {
                              plies: formatNumber(option.plies),
                            })}{' '}
                            ·{' '}
                            {t(
                              option.game_no != null &&
                                option.status === 'completed'
                                ? 'gameAnalysis.completed'
                                : option.game_no != null &&
                                    option.status === 'failed'
                                  ? 'gameAnalysis.failed'
                                  : statusLabels[option.status],
                            )}{' '}
                            {gameNo !== undefined && option.game_no == null
                              ? `· ${text('compareFromMatch')} `
                              : ''}
                            · {date.format(new Date(option.created_at))}
                          </option>
                        ))}
                      </select>
                      <small>
                        <EngineLabel
                          details={
                            job.engine_snapshot?.engine_version ??
                            summary?.engine
                          }
                        />
                      </small>
                      <small>
                        {gameNo !== undefined && job.game_no == null
                          ? text('compareFromMatch')
                          : text('coverage', {
                              completed: formatNumber(job.completed_games),
                              total: formatNumber(job.total_games),
                            })}
                      </small>
                      {job.error && (
                        <small>
                          {t('error.engine', { details: job.error })}
                        </small>
                      )}
                    </>
                  ) : (
                    <small>
                      {text(
                        gameNo === undefined
                          ? 'compareNoRun'
                          : 'compareNoGameRun',
                      )}
                    </small>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          {names.map((name, player) => {
            const stats = columns.map(({ summary }) =>
              summary?.players.find((entry) => entry.player === player),
            );
            const rows = [
              [
                text('compareCheckerCount'),
                stats.map(
                  (entry) => entry && formatNumber(entry.checker_decisions),
                ),
              ],
              [
                text('compareCubeCount'),
                stats.map(
                  (entry) => entry && formatNumber(entry.cube_decisions),
                ),
              ],
              [
                text('compareCheckerLoss'),
                stats.map(
                  (entry) => entry && decimal(entry.checker_equity_loss),
                ),
              ],
              [
                text('compareCubeLoss'),
                stats.map((entry) => entry && decimal(entry.cube_equity_loss)),
              ],
              [
                text('compareUnavailable'),
                stats.map((entry) => entry && formatNumber(entry.unavailable)),
              ],
              [
                'GNUbg PR',
                columns.map(({ engine, summary }) => {
                  const rating =
                    engine.id === 'gnubg' && summary
                      ? nativePerformanceRating(summary, player)
                      : undefined;
                  return rating === undefined ? undefined : decimal(rating);
                }),
              ],
            ] as const;
            return (
              <tbody key={player}>
                <tr className="statistics-comparison-player">
                  <th scope="rowgroup" colSpan={3}>
                    {name}
                  </th>
                </tr>
                {rows.map(([label, values]) => (
                  <tr key={label}>
                    <th scope="row">{label}</th>
                    {values.map((value, column) => (
                      <td key={columns[column].engine.id}>
                        {value ?? unavailable}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            );
          })}
        </table>
      </div>
      <p className="analysis-method">{text('compareMethod')}</p>
    </>
  );
}

export default function MatchAnalysisComparison({
  matchId,
  gameNo,
  names,
  visible,
}: {
  matchId: string;
  gameNo?: number;
  names: string[];
  visible: boolean;
}) {
  const { formatError } = useI18n();
  const text = useStatisticsText();
  const [open, setOpen] = useState(false);
  const [history, setHistory] = useState<{
    url: string;
    jobs: ComparisonRun[];
  }>();
  const [failure, setFailure] = useState<{ url: string; error: unknown }>();
  const url = `/matches/${matchId}/analysis/jobs${gameNo === undefined ? '' : `?game_no=${gameNo}`}`;
  const jobs = history?.url === url ? history.jobs : undefined;
  const error = failure?.url === url ? failure.error : undefined;

  useEffect(() => {
    if (!open || !visible) return;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    async function refresh() {
      try {
        const jobs = await request<ComparisonRun[]>(url, {
          signal: controller.signal,
        });
        if (controller.signal.aborted) return;
        setHistory({ url, jobs });
        setFailure(undefined);
      } catch (error) {
        if (!controller.signal.aborted) setFailure({ url, error });
      } finally {
        if (!controller.signal.aborted) timer = setTimeout(refresh, 5000);
      }
    }
    void refresh();
    return () => {
      controller.abort();
      clearTimeout(timer);
    };
  }, [url, open, visible]);

  return (
    <details
      className="statistics-comparison"
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary>
        {text(gameNo === undefined ? 'compareEngines' : 'compareGameEngines')}
      </summary>
      {!!error && <p role="alert">{formatError(error)}</p>}
      {open && !jobs && !error && <p role="status">{text('loading')}</p>}
      {jobs && (
        <MatchAnalysisComparisonResults
          key={url}
          jobs={jobs}
          names={names}
          gameNo={gameNo}
        />
      )}
    </details>
  );
}
