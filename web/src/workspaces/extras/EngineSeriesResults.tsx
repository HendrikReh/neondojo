import ActionButton from '../../shared/ui/ActionButton';
import EngineLabel from '../../shared/engine/EngineLabel';
import { formatPly } from '../../shared/format/ply';
import { useI18n } from '../../shared/i18n/i18n';
import { formatBenchmarkDuration } from './benchmark-duration';
import {
  seriesSgf,
  type SeriesDetails,
  type SideCostSummary,
} from './engine-match-api';
import {
  isActive,
  isMoney,
  progressLabel,
  statusLabel,
  unitLabel,
  type Competitor,
  type Text,
} from './engine-series';

/** Progress, statistics and stored matches of one series. */
export default function EngineSeriesResults({
  details,
  setOffset,
  paging,
  text,
  profile,
  busy,
  opening,
  onOpen,
  onCancel,
  onResume,
  onDelete,
  onReuse,
}: {
  details: SeriesDetails;
  setOffset: (offset: number) => void;
  paging: boolean;
  text: Text;
  profile: (side: Competitor) => string;
  busy: boolean;
  /** Index of the match whose library entry is being opened. */
  opening: number | undefined;
  onOpen: (matchIndex: number) => void;
  onCancel: () => void;
  onResume: () => void;
  onDelete: () => void;
  onReuse: (trigger: HTMLButtonElement) => void;
}) {
  const { formatNumber, formatPercent } = useI18n();
  const { series, statistics, matches, next_offset, previous_offset } = details;
  const { configuration: config, engines } = series;
  const paired = config.mode === 'paired';
  const moneyDuel = isMoney(config);
  const money = statistics.money;
  const rate = statistics.win_rate_a;
  const interval = statistics.confidence_interval_a;
  const playing = isActive(series.status);
  const resumable =
    series.status === 'cancelled' &&
    series.completed_matches < series.total_matches;
  const live = series.status === 'running' ? details.live_progress : undefined;
  const blockGames = moneyDuel ? config.block_games : undefined;
  const completedUnits =
    series.completed_matches +
    (live && blockGames ? live.completed_games / blockGames : 0);
  // Stored units (matches or blocks) in the statistics: complete pairs only.
  const counted = paired
    ? statistics.complete_pairs * 2
    : series.completed_matches;
  const signed = (value: number) =>
    formatNumber(value, {
      signDisplay: 'exceptZero',
      minimumFractionDigits: 3,
      maximumFractionDigits: 3,
    });
  const share = (part: number) =>
    money && money.games > 0 ? formatPercent(part / money.games, 1) : '—';
  const costs = [statistics.cost_a ?? null, statistics.cost_b ?? null] as const;
  const duration = (milliseconds: number) =>
    formatBenchmarkDuration(milliseconds, formatNumber);
  const perSide = (format: (cost: SideCostSummary) => string, missing = '—') =>
    costs
      .map(
        (cost, side) => `${side ? 'B' : 'A'} ${cost ? format(cost) : missing}`,
      )
      .join(' · ');
  const stoppedEmpty = !playing && counted === 0 && matches.length === 0;
  const competitors = [
    { side: 'A', profile: config.a, rate, wins: statistics.wins_a },
    {
      side: 'B',
      profile: config.b,
      rate: rate == null ? rate : 1 - rate,
      wins: statistics.wins_b,
    },
  ];
  return (
    <>
      <div className="engine-match-progress" data-active={playing}>
        <p role="status">
          {statusLabel(series.status, text)} ·{' '}
          {progressLabel(series, text, formatNumber)}
        </p>
        {playing && (
          <>
            <progress
              aria-label={text(moneyDuel ? 'blocks' : 'matches')}
              value={completedUnits}
              max={series.total_matches}
            />
            <ActionButton
              icon="cancel"
              type="button"
              label={text('cancel')}
              disabled={busy}
              onClick={onCancel}
            />
          </>
        )}
        {resumable && (
          <ActionButton
            icon="play"
            type="button"
            className="primary"
            label={text('resume')}
            disabled={busy}
            onClick={onResume}
          />
        )}
        {!playing && (
          <ActionButton
            icon="delete"
            type="button"
            label={text('deleteSeries')}
            aria-haspopup="dialog"
            disabled={busy}
            onClick={onDelete}
          />
        )}
        <ActionButton
          icon="copy"
          type="button"
          label={text('reuse')}
          aria-haspopup="dialog"
          onClick={(event) => onReuse(event.currentTarget)}
        />
        {live && (
          <div className="engine-match-live-progress">
            <span>
              {text(moneyDuel ? 'currentBlock' : 'currentMatch', {
                current: live.unit_number,
                total: series.total_matches,
              })}
            </span>
            <span>
              {blockGames
                ? text('currentBlockGame', {
                    current: live.game_number,
                    total: blockGames,
                  })
                : text('currentGame', { current: live.game_number })}
            </span>
            <span>
              {text('currentScore', {
                a: live.score_a,
                b: live.score_b,
              })}
            </span>
            <span>
              {live.estimated_remaining_seconds == null
                ? text('estimatingTime')
                : text('remainingTime', {
                    time: duration(live.estimated_remaining_seconds * 1_000),
                  })}
            </span>
          </div>
        )}
      </div>
      {series.status === 'cancelled' && !!series.saved_games_in_unit && (
        <p>{text('savedGames', { games: series.saved_games_in_unit })}</p>
      )}
      {(series.status === 'failed' || series.error) && (
        <div className="engine-match-failure error">
          {series.status === 'failed' && (
            <p role="alert">
              {text('failedSummary')}
              {series.completed_matches > 0 && ` ${text('partialResults')}`}
            </p>
          )}
          {series.error && (
            <details>
              <summary>{text('details')}</summary>
              <p>{series.error}</p>
            </details>
          )}
        </div>
      )}
      {!stoppedEmpty && (
        <>
          <div className="engine-match-win-rates">
            {competitors.map((competitor) => (
              <dl key={competitor.side}>
                <div className="engine-match-rate">
                  <dt>
                    <strong>
                      {competitor.side} · {profile(competitor.profile)}
                    </strong>
                    <span>{text(moneyDuel ? 'gameWinRate' : 'winRate')}</span>
                  </dt>
                  <dd>
                    {competitor.rate == null
                      ? '—'
                      : formatPercent(competitor.rate, 1)}
                  </dd>
                </div>
                <div className="engine-match-rate-count">
                  <dt>{text(moneyDuel ? 'gamesWon' : 'matchWins')}</dt>
                  <dd>{formatNumber(competitor.wins)}</dd>
                </div>
              </dl>
            ))}
            {rate != null && (
              <div className="engine-match-share" aria-hidden="true">
                <span style={{ inlineSize: `${rate * 100}%` }} />
              </div>
            )}
          </div>
          <dl className="engine-match-statistics">
            <div>
              <dt>{text(moneyDuel ? 'blocksCounted' : 'matchesCounted')}</dt>
              <dd>
                {formatNumber(counted)}
                {money && (
                  <small>
                    {text('games')} {formatNumber(money.games)}
                  </small>
                )}
              </dd>
            </div>
            {paired && (
              <div>
                <dt>{text('completePairs')}</dt>
                <dd>
                  {formatNumber(statistics.complete_pairs)} /{' '}
                  {formatNumber(config.count)}
                </dd>
              </div>
            )}
            {costs.some(Boolean) && (
              <div>
                <dt title={text('cpuPerMoveHint')}>{text('cpuPerMove')}</dt>
                <dd>
                  {perSide((cost) => duration(cost.cpu_ms_per_move))}
                  <small>
                    {text('wallPerMove')}{' '}
                    {perSide((cost) => duration(cost.wall_ms_per_move))}
                  </small>
                  {costs.some((cost) => cost?.measured_matches !== counted) && (
                    <small>
                      {text(moneyDuel ? 'blocksMeasured' : 'costMeasured')}{' '}
                      {perSide(
                        (cost) =>
                          `${formatNumber(cost.measured_matches)} / ${formatNumber(counted)}`,
                        formatNumber(0),
                      )}
                    </small>
                  )}
                </dd>
              </div>
            )}
            {money?.points_per_game_a != null && (
              <div>
                <dt>{text('pointsPerGame')}</dt>
                <dd>
                  {signed(money.points_per_game_a)}
                  {money.confidence_interval && (
                    <small>
                      {text('interval')}:{' '}
                      {money.confidence_interval.map(signed).join(' – ')} ·{' '}
                      {text(
                        money.confidence_interval[0] <= 0 &&
                          money.confidence_interval[1] >= 0
                          ? 'ppgInconclusive'
                          : 'ppgClear',
                      )}
                    </small>
                  )}
                </dd>
              </div>
            )}
            {money && money.games > 0 && (
              <div>
                <dt>{text('gammonShare')}</dt>
                <dd>
                  A {share(money.outcomes_a.gammon)} /{' '}
                  {share(money.outcomes_a.backgammon)} · B{' '}
                  {share(money.outcomes_b.gammon)} /{' '}
                  {share(money.outcomes_b.backgammon)}
                </dd>
              </div>
            )}
            {interval && (
              <div>
                <dt>{text('interval')}</dt>
                <dd>
                  {interval.map((value) => formatPercent(value, 1)).join(' – ')}
                  <small>
                    {text(
                      interval[0] <= 0.5 && interval[1] >= 0.5
                        ? 'intervalInconclusive'
                        : 'intervalClear',
                    )}
                  </small>
                </dd>
              </div>
            )}
          </dl>
        </>
      )}
      <details className="engine-match-settings">
        <summary>{text('seriesSettings')}</summary>
        <p>
          {moneyDuel
            ? `${text('formatMoney')} · ${text(config.mode)} · ${text('blockGames')} ${formatNumber(config.block_games ?? 0)}`
            : `${text(config.mode)} · ${text('length')} ${formatNumber(config.match_length)}`}
        </p>
        <p>
          {text('seed')} <code>{config.seed}</code> · {series.dice_version}
        </p>
        <p>
          {text('engineA')} ·{' '}
          <EngineLabel
            details={engines.a.engine_version ?? profile(config.a)}
            tooltip={
              engines.a.engine_version
                ? `${engines.a.engine_version} · ${formatPly(config.a.plies, formatNumber)}`
                : undefined
            }
          />
        </p>
        <p>
          {text('engineB')} ·{' '}
          <EngineLabel
            details={engines.b.engine_version ?? profile(config.b)}
            tooltip={
              engines.b.engine_version
                ? `${engines.b.engine_version} · ${formatPly(config.b.plies, formatNumber)}`
                : undefined
            }
          />
        </p>
      </details>
      {stoppedEmpty ? null : matches.length === 0 ? (
        <p>{text('noMatches')}</p>
      ) : (
        <div className="benchmark-table-scroll" aria-busy={paging}>
          <table>
            <thead>
              <tr>
                <th>{text(unitLabel(moneyDuel, paired))}</th>
                <th>{text('white')}</th>
                <th>{text(moneyDuel ? 'points' : 'score')}</th>
                <th>{text('games')}</th>
                <th>{text('seed')}</th>
                <th>
                  <span className="sr-only">{text('actions')}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {matches.map((match) => {
                const isReturn = paired && match.match_index % 2 === 1;
                const incomplete =
                  paired &&
                  !isReturn &&
                  match.match_index + 1 === series.completed_matches;
                return (
                  <tr
                    key={match.match_index}
                    data-leg={
                      paired ? (isReturn ? 'return' : 'outbound') : undefined
                    }
                  >
                    <th scope="row">
                      {formatNumber(
                        paired
                          ? Math.floor(match.match_index / 2) + 1
                          : match.match_index + 1,
                      )}
                      {paired && ` · ${text(isReturn ? 'return' : 'outbound')}`}
                      {incomplete && <small>{text('incompletePair')}</small>}
                    </th>
                    <td>{isReturn ? 'B' : 'A'}</td>
                    <td>
                      {formatNumber(match.score_a)}–
                      {formatNumber(match.score_b)}
                    </td>
                    <td>{formatNumber(match.games)}</td>
                    <td className="engine-match-seed">
                      <code>{match.seed}</code>
                    </td>
                    <td className="engine-match-actions">
                      <ActionButton
                        icon="analysis"
                        type="button"
                        label={text('analysis')}
                        tooltip={text('openInAnalysis')}
                        aria-disabled={opening !== undefined || undefined}
                        aria-busy={opening === match.match_index || undefined}
                        onClick={() => {
                          if (opening === undefined) onOpen(match.match_index);
                        }}
                      />
                      <a
                        href={seriesSgf(series.id, match.match_index)}
                        download
                      >
                        SGF
                      </a>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {(previous_offset != null || next_offset != null) && (
        <div className="engine-match-pagination">
          <ActionButton
            icon="back"
            type="button"
            label={text('previous')}
            disabled={previous_offset == null}
            aria-disabled={paging || undefined}
            onClick={() => {
              if (!paging && previous_offset != null)
                setOffset(previous_offset);
            }}
          />
          <ActionButton
            icon="next"
            type="button"
            label={text('next')}
            disabled={next_offset == null}
            aria-disabled={paging || undefined}
            onClick={() => {
              if (!paging && next_offset != null) setOffset(next_offset);
            }}
          />
        </div>
      )}
    </>
  );
}
