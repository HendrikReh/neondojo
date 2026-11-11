import ActionButton from './ActionButton';
import { useId } from 'react';
import { usePlayText } from './play-messages';
type OpeningRollInfo =
  import('./generated-api').components['schemas']['OpeningRoll'];
export default function PlayOpeningRoll({
  stage,
  opening,
  white,
  black,
  busy,
  onRoll,
  onContinue,
  tied,
}: {
  stage: 'opening_roll' | 'opening_result';
  tied?: boolean;
  opening?: OpeningRollInfo | null;
  white: string;
  black: string;
  busy: boolean;
  onRoll: () => void;
  onContinue: () => void;
}) {
  const t = usePlayText(),
    id = useId();
  const result = stage === 'opening_result' && opening;
  const rolls = result ? opening.rolls : [];
  return (
    <section className="play-opening" aria-labelledby={id}>
      <h3 id={id}>{t('openingTitle')}</h3>
      <p>{t('openingRule')}</p>
      {tied && <p role="status">{t('openingTie')}</p>}
      {rolls.length > 1 && (
        <ol className="play-opening-ties" aria-label={t('openingAttempts')}>
          {rolls.slice(0, -1).map((roll, index) => (
            <li key={index}>
              {white}: {roll[0]} · {black}: {roll[1]} — {t('openingTie')}
            </li>
          ))}
        </ol>
      )}
      {result && (
        <p className="play-opening-winner" role="status">
          <strong>{opening.winner === 0 ? white : black}</strong>{' '}
          {t('openingWins')}{' '}
          <strong>
            {Math.max(opening.white_die, opening.black_die)} {t('openingAnd')}{' '}
            {Math.min(opening.white_die, opening.black_die)}
          </strong>
          . {t('openingSameDice')}
        </p>
      )}
      <div className="play-actions">
        {stage === 'opening_roll' ? (
          <ActionButton
            type="button"
            disabled={busy}
            onClick={onRoll}
            icon="roll"
            label={t('openingThrow')}
          >
            {t('openingThrow')}
          </ActionButton>
        ) : (
          <ActionButton
            type="button"
            disabled={busy || !result}
            onClick={onContinue}
            icon="play"
            label={t('openingContinue')}
          >
            {t('openingContinue')}
          </ActionButton>
        )}
      </div>
    </section>
  );
}
