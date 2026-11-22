import { useI18n } from '../../shared/i18n/i18n';
import { gameResultText } from './game-result-messages';
import './game-result.css';
type Schema = import('../../generated-api').components['schemas'];
export type GameResult = Schema['GameResult'];
export type PlayGameResult = Schema['PlayGameResult'];
export default function GameResultSummary({
  result,
  white,
  black,
}: {
  result: GameResult;
  white: string;
  black: string;
}) {
  const { locale, formatNumber } = useI18n();
  const t = (
    key: Parameters<typeof gameResultText>[1],
    values?: Record<string, string | number>,
  ) => gameResultText(locale, key, values);
  const points = `${formatNumber(result.points)} ${t(result.points === 1 ? 'point' : 'points')}`;
  const cube = `${t('cube')} ${formatNumber(result.cube)}`;
  const reason = t(result.reason);
  const winningScore = result.scores_after[result.winner];
  const calculation =
    result.reason === 'drop'
      ? `${reason} · ${cube} = ${points}`
      : result.multiplier != null
        ? `${reason} (${formatNumber(result.multiplier)}) × ${cube} = ${points}`
        : `${reason} · ${formatNumber(winningScore)} − ${formatNumber(winningScore - result.points)} = ${points}`;
  return (
    <section
      className="game-result-summary"
      data-testid="game-result"
      aria-label={t('heading')}
    >
      <strong>
        {t(result.points === 1 ? 'winnerOne' : 'winner', {
          player: result.winner === 0 ? white : black,
          points: formatNumber(result.points),
        })}
      </strong>
      <p>{calculation}</p>
      <p className="game-result-score">{`${t('score')}: ${white} ${formatNumber(result.scores_after[0])} : ${formatNumber(result.scores_after[1])} ${black}`}</p>
    </section>
  );
}
