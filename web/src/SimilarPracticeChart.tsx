import { useRef, useState, type KeyboardEvent } from 'react';
import Tooltip from './shared/ui/Tooltip';
import { useI18n } from './shared/i18n/i18n';
import { usePositionText } from './position-browser-messages';
import type { PracticeHistoryPage } from './similar-practice-history-api';

const tickFractions = [1, 0.75, 0.5, 0.25, 0];
const rollingWindows = [5, 10, 20];

export default function SimilarPracticeChart({
  items,
}: {
  items: PracticeHistoryPage['items'];
}) {
  const p = usePositionText();
  const { locale, formatNumber } = useI18n();
  const [rollingWindow, setRollingWindow] = useState(rollingWindows[0]);
  const [focusedId, setFocusedId] = useState<string>();
  const markers = useRef(new Map<string, HTMLSpanElement>());
  if (!items.length) return null;

  // History arrives newest first, including pages appended by Load older rounds.
  const rounds = [...items].reverse().map((round) => ({
    ...round,
    time: Date.parse(round.completed_at),
  }));
  const first = rounds[0];
  const last = rounds[rounds.length - 1];
  const duration = last.time - first.time;
  const maxLoss = rounds.reduce(
    (max, round) => Math.max(max, round.average_equity_loss),
    0.001,
  );
  const unit = 10 ** Math.floor(Math.log10(maxLoss));
  const roundedMax = Math.ceil(maxLoss / unit) * unit;
  const axisMax = Number.isFinite(roundedMax)
    ? Math.max(maxLoss, roundedMax)
    : maxLoss;
  const scaleLoss = (loss: number) => (1 - loss / axisMax) * 100;
  const points = rounds.map((round) => ({
    round,
    x: duration ? ((round.time - first.time) / duration) * 100 : 50,
    y: scaleLoss(round.average_equity_loss),
  }));
  const originals = points.filter(({ round }) => !round.is_retry);
  const line = originals.map(({ x, y }) => `${x},${y}`).join(' ');
  const trend = originals.map(({ round, x }, index) => {
    const window = originals.slice(
      Math.max(0, index + 1 - rollingWindow),
      index + 1,
    );
    const average = window.reduce(
      (mean, { round }, i) =>
        mean + (round.average_equity_loss - mean) / (i + 1),
      0,
    );
    return { id: round.id, x, average };
  });
  const trendLine = trend
    .map(({ x, average }) => `${x},${scaleLoss(average)}`)
    .join(' ');
  const averages = new Map(trend.map(({ id, average }) => [id, average]));
  const showTrend = trend.length > 1;
  const trendLabel = p('practiceRollingAverage', { count: rollingWindow });
  const lossFormat = { minimumFractionDigits: 3, maximumFractionDigits: 6 };
  const selectedId = rounds.some(({ id }) => id === focusedId)
    ? focusedId
    : last.id;
  const day = new Intl.DateTimeFormat(locale, { dateStyle: 'medium' });
  const dateTime = new Intl.DateTimeFormat(locale, {
    dateStyle: 'medium',
    timeStyle: 'medium',
  });
  const formatDate =
    day.format(first.time) === day.format(last.time)
      ? new Intl.DateTimeFormat(locale, {
          dateStyle: 'medium',
          timeStyle: duration < 60_000 ? 'medium' : 'short',
        })
      : day;
  const dateTicks =
    formatDate.format(first.time) === formatDate.format(last.time)
      ? [first]
      : [first, last];

  function moveFocus(event: KeyboardEvent<HTMLSpanElement>, index: number) {
    let next: number;
    if (event.key === 'ArrowLeft') next = Math.max(0, index - 1);
    else if (event.key === 'ArrowRight')
      next = Math.min(points.length - 1, index + 1);
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = points.length - 1;
    else return;
    event.preventDefault();
    markers.current.get(points[next].round.id)?.focus();
  }

  return (
    <figure
      className="practice-history-chart"
      aria-label={p('practiceLossOverTime')}
    >
      <figcaption>
        <div className="practice-chart-heading">
          <strong>{p('practiceLossOverTime')}</strong>
          {showTrend && (
            <label className="practice-chart-window">
              {p('practiceRollingWindow')}
              <select
                value={rollingWindow}
                onChange={(event) =>
                  setRollingWindow(Number(event.currentTarget.value))
                }
              >
                {rollingWindows.map((count) => (
                  <option key={count} value={count}>
                    {p('practiceRollingWindowRounds', { count })}
                  </option>
                ))}
              </select>
            </label>
          )}
        </div>
        <ul className="practice-chart-legend">
          <li>
            <span className="practice-chart-symbol" aria-hidden="true" />
            {p('practiceRoundInitial')}
          </li>
          <li>
            <span
              className="practice-chart-symbol practice-chart-symbol--retry"
              aria-hidden="true"
            />
            {p('practiceRoundRetry')}
          </li>
          {showTrend && (
            <li>
              <span className="practice-chart-rolling-key" aria-hidden="true" />
              {trendLabel}
            </li>
          )}
        </ul>
      </figcaption>
      <div className="practice-chart-grid">
        <div className="practice-chart-scale" aria-hidden="true">
          {tickFractions.map((fraction) => (
            <span key={fraction} style={{ top: `${(1 - fraction) * 100}%` }}>
              {formatNumber(axisMax * fraction, {
                maximumFractionDigits: 6,
                notation:
                  axisMax >= 1000 && fraction ? 'scientific' : 'standard',
              })}
            </span>
          ))}
        </div>
        <div className="practice-chart-plot">
          <svg
            viewBox="0 0 100 100"
            preserveAspectRatio="none"
            aria-hidden="true"
          >
            {tickFractions.map((fraction) => (
              <line
                key={fraction}
                x1={0}
                x2={100}
                y1={(1 - fraction) * 100}
                y2={(1 - fraction) * 100}
                vectorEffect="non-scaling-stroke"
              />
            ))}
            {originals.length > 1 && (
              <polyline
                className="practice-chart-original"
                points={line}
                vectorEffect="non-scaling-stroke"
              />
            )}
            {showTrend && (
              <polyline
                className="practice-chart-rolling"
                points={trendLine}
                vectorEffect="non-scaling-stroke"
              />
            )}
          </svg>
          {points.map(({ round, x, y }, index) => {
            const loss = formatNumber(round.average_equity_loss, lossFormat);
            const kind = p(
              round.is_retry ? 'practiceRoundRetry' : 'practiceRoundInitial',
            );
            const markerLabel = `${kind} · ${dateTime.format(round.time)} · ${loss}`;
            const average = averages.get(round.id);
            const label =
              showTrend && average !== undefined
                ? `${markerLabel} · ${trendLabel} · ${formatNumber(average, lossFormat)}`
                : markerLabel;
            return (
              <Tooltip
                key={round.id}
                as="span"
                ref={(node) => {
                  if (node) markers.current.set(round.id, node);
                  else markers.current.delete(round.id);
                }}
                role="img"
                aria-label={label}
                text={label}
                tabIndex={round.id === selectedId ? 0 : -1}
                className="practice-chart-point"
                style={{ left: `${x}%`, top: `${y}%` }}
                onFocus={() => setFocusedId(round.id)}
                onKeyDown={(event) => moveFocus(event, index)}
              >
                <span
                  className={`practice-chart-symbol${round.is_retry ? ' practice-chart-symbol--retry' : ''}`}
                  aria-hidden="true"
                />
              </Tooltip>
            );
          })}
        </div>
        <div className="practice-chart-dates" aria-hidden="true">
          {dateTicks.map((round) => (
            <time key={round.id} dateTime={round.completed_at}>
              {formatDate.format(round.time)}
            </time>
          ))}
        </div>
      </div>
    </figure>
  );
}
