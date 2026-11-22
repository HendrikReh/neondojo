import { useI18n } from '../../shared/i18n/i18n';
import type { components } from '../../generated-api';
import { usePlayerText } from './player-directory-messages';
import { prDigits } from '../../shared/format/pr-format';
import Tooltip from '../../shared/ui/Tooltip';
import { dateTimeFormat } from '../../shared/i18n/i18n-core';

type Point = components['schemas']['PlayerRatingPoint'];
export const RATING_WINDOW = 10;

/**
 * Rating of the last `size` matches up to each match, weighted by the
 * number of rated decisions so a short match does not dominate.
 */
export function rollingRatings(points: Point[], size = RATING_WINDOW) {
  return points.map((_, index) => {
    const recent = points.slice(Math.max(0, index + 1 - size), index + 1);
    const decisions = recent.reduce((sum, p) => sum + p.decisions, 0);
    return decisions > 0
      ? recent.reduce((sum, p) => sum + p.value * p.decisions, 0) / decisions
      : recent.reduce((sum, p) => sum + p.value, 0) / recent.length;
  });
}

/** Plain rolling mean of the values that exist; null until one does. */
export function rollingMean(
  values: (number | null | undefined)[],
  size = RATING_WINDOW,
) {
  return values.map((_, index) => {
    const recent = values
      .slice(Math.max(0, index + 1 - size), index + 1)
      .filter((value): value is number => value != null);
    return recent.length
      ? recent.reduce((sum, value) => sum + value, 0) / recent.length
      : null;
  });
}

/** Ratings of one analysis profile, oldest first, with a rolling average. */
export default function PlayerRatingTrend({ points }: { points: Point[] }) {
  const t = usePlayerText();
  const { locale, formatNumber } = useI18n();
  if (points.length < 2) return null;
  const averages = rollingRatings(points);
  const checker = rollingMean(points.map((p) => p.checker_value));
  const cube = rollingMean(points.map((p) => p.cube_value));
  const max = Math.max(
    ...points.map((p) => p.value),
    ...averages,
    ...[...checker, ...cube].filter((value): value is number => value != null),
    1,
  );
  const top = Math.ceil(max / 5) * 5;
  const x = (index: number) => (index / (points.length - 1)) * 100;
  const y = (value: number) => (1 - value / top) * 100;
  const date = dateTimeFormat(locale, { dateStyle: 'medium' });
  const day = (point: Point) => date.format(new Date(`${point.date}T12:00:00`));
  const pr = (value: number) => formatNumber(value, prDigits);
  const line = (values: (number | null)[]) =>
    values
      .flatMap((value, i) => (value == null ? [] : [`${x(i)},${y(value)}`]))
      .join(' ');
  return (
    <figure className="player-rating-trend" aria-label={t('prTrend')}>
      <figcaption>
        <strong>{t('prTrend')}</strong>
        <span className="fineprint">
          {t('prTrendHint', { count: RATING_WINDOW })}
        </span>
        <ul className="player-rating-legend">
          <li className="player-rating-key player-rating-key--total">
            {t('prTrendTotal')}
          </li>
          <li className="player-rating-key player-rating-key--checker">
            {t('prTrendChecker')}
          </li>
          <li className="player-rating-key player-rating-key--cube">
            {t('prTrendCube')}
          </li>
        </ul>
      </figcaption>
      <div className="player-rating-plot">
        <span className="player-rating-axis" aria-hidden="true">
          {formatNumber(top)}
        </span>
        <span
          className="player-rating-axis player-rating-axis--zero"
          aria-hidden="true"
        >
          0
        </span>
        <svg
          viewBox="0 0 100 100"
          preserveAspectRatio="none"
          aria-hidden="true"
        >
          <polyline
            className="player-rating-split player-rating-split--checker"
            points={line(checker)}
            vectorEffect="non-scaling-stroke"
          />
          <polyline
            className="player-rating-split player-rating-split--cube"
            points={line(cube)}
            vectorEffect="non-scaling-stroke"
          />
          <polyline
            className="player-rating-average"
            points={averages.map((value, i) => `${x(i)},${y(value)}`).join(' ')}
            vectorEffect="non-scaling-stroke"
          />
        </svg>
        {points.map((point, index) => {
          const label = t('prTrendPoint', {
            date: day(point),
            value: pr(point.value),
            count: RATING_WINDOW,
            average: pr(averages[index]),
          });
          const split = [
            point.checker_value != null &&
              `${t('prTrendChecker')} ${pr(point.checker_value)}`,
            point.cube_value != null &&
              `${t('prTrendCube')} ${pr(point.cube_value)}`,
          ]
            .filter(Boolean)
            .join(' · ');
          const text = split ? `${label} · ${split}` : label;
          return (
            <Tooltip
              key={point.match_id}
              as="span"
              role="img"
              aria-label={text}
              text={text}
              tabIndex={0}
              className="player-rating-point"
              style={{ left: `${x(index)}%`, top: `${y(point.value)}%` }}
            />
          );
        })}
      </div>
      <div className="player-rating-dates" aria-hidden="true">
        <time>{day(points[0])}</time>
        <time>{day(points[points.length - 1])}</time>
      </div>
    </figure>
  );
}
