import { useEffect, useState } from 'react';
import { useI18n } from '../i18n/i18n';
import {
  loadMoveExplanation,
  moveDifferences,
  sixChances,
  type ExplainedPosition,
  type MoveExplanation as Explanation,
} from './move-explanation';
import { useMoveExplanationText } from './move-explanation-messages';
import './move-explanation.css';

/** Terms a newcomer needs to read an evaluation. */
export function Glossary() {
  const t = useMoveExplanationText();
  return (
    <details className="evaluation-glossary">
      <summary>{t('glossary')}</summary>
      <ul>
        {(['equity', 'loss', 'pr', 'ply'] as const).map((term) => (
          <li key={term}>{t(term)}</li>
        ))}
      </ul>
    </details>
  );
}

/**
 * Board facts that differ between the best play and another play. Hidden
 * while the request runs or fails, so it never blocks the evaluation itself.
 */
export default function MoveExplanation({
  position,
  best,
  played,
  bestChances,
  playedChances,
}: {
  position: ExplainedPosition;
  best: string;
  played: string;
  bestChances?: unknown;
  playedChances?: unknown;
}) {
  const t = useMoveExplanationText();
  const { formatDecision, formatNumber, formatPercent } = useI18n();
  const key = JSON.stringify([position.board, position.player, position.dice]);
  const [state, setState] = useState<{
    key: string;
    explanation: Readonly<Explanation>;
  }>();
  const request = `${key}|${best}|${played}`;
  useEffect(() => {
    if (!best || !played || best === played || position.dice.length !== 2)
      return;
    const abort = new AbortController();
    void loadMoveExplanation(position, best, played, abort.signal)
      .then((explanation) => {
        if (!abort.signal.aborted) setState({ key: request, explanation });
      })
      .catch(() => undefined);
    return () => abort.abort();
    // The request string covers the position, both plays and the dice.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request]);
  if (state?.key !== request || best === played) return null;
  const differences = moveDifferences(
    state.explanation,
    sixChances(bestChances),
    sixChances(playedChances),
  );
  const describe = (difference: (typeof differences)[number]) => {
    if ('points' in difference)
      return t(difference.kind, {
        points: difference.points
          .map((point) => formatNumber(point))
          .join(', '),
      });
    const percent = ['win', 'gammon', 'gammonLoss'].includes(difference.kind);
    const value = (number: number) =>
      percent ? formatPercent(number, 1) : formatNumber(number);
    const pips = (number: number) =>
      formatNumber(number, {
        minimumFractionDigits: 1,
        maximumFractionDigits: 1,
      });
    return t(difference.kind, {
      best: value(difference.best),
      played: value(difference.played),
      ...(difference.kind === 'shots'
        ? {
            bestCost: pips(difference.bestCost),
            playedCost: pips(difference.playedCost),
          }
        : {}),
    });
  };
  const list = (favored: 'best' | 'played') => {
    const items = differences.filter(
      (difference) => difference.favors === favored,
    );
    return (
      items.length > 0 && (
        <div data-explanation-favors={favored}>
          <h5>{t(favored === 'best' ? 'forBest' : 'forPlayed')}</h5>
          <ul>
            {items.map((difference) => (
              <li key={difference.kind}>{describe(difference)}</li>
            ))}
          </ul>
        </div>
      )
    );
  };
  const forBest = list('best');
  return (
    <section className="move-explanation" data-move-explanation>
      <h4>{t('title')}</h4>
      <p className="fineprint">
        {t('columns', {
          best: formatDecision(best),
          played: formatDecision(played),
        })}
      </p>
      {differences.length ? (
        <>
          {forBest || <p>{t('onlyEvaluation')}</p>}
          {list('played')}
        </>
      ) : (
        <p>{t('same')}</p>
      )}
      <p className="fineprint">{t('note')}</p>
      <Glossary />
    </section>
  );
}
