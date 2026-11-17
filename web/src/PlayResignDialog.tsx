import { useId, useState, type ReactNode, type RefObject } from 'react';
import ActionButton from './ActionButton';
import PlayBoardDialog from './PlayBoardDialog';
import { useI18n } from './i18n';
import { gameResultText } from './game-result-messages';
import { usePlayResignText } from './play-resign-messages';
export type ResignLevel = 1 | 2 | 3;
export default function PlayResignDialog({
  anchor,
  cube,
  white,
  black,
  humanPlayer,
  scores,
  matchLength,
  busy,
  error,
  onConfirm,
  onForfeit,
  onClose,
}: {
  anchor: RefObject<HTMLDivElement | null>;
  cube: number;
  white: string;
  black: string;
  humanPlayer: number;
  scores: [number, number];
  matchLength: number;
  busy: boolean;
  error: ReactNode;
  onConfirm: (level: ResignLevel) => void;
  /** Opens the match forfeit confirmation; only offered in point matches. */
  onForfeit?: () => void;
  onClose: () => void;
}) {
  const [level, setLevel] = useState<ResignLevel>(1);
  const id = useId();
  const t = usePlayResignText();
  const { locale, formatNumber, t: appT } = useI18n();
  const opponent = humanPlayer === 0 ? black : white;
  const points = level * cube;
  const projectedScores = [...scores];
  projectedScores[1 - humanPlayer] += points;
  const endsMatch =
    matchLength > 0 && projectedScores[1 - humanPlayer] >= matchLength;
  return (
    <PlayBoardDialog
      anchor={anchor}
      heading={t('heading')}
      busy={busy}
      onClose={onClose}
      className="play-resign-dialog"
    >
      <p>{t('help')}</p>
      {error}
      <fieldset className="play-resign-levels" disabled={busy}>
        <legend>{t('level')}</legend>
        {([1, 2, 3] as const).map((value) => (
          <label key={value}>
            <input
              type="radio"
              name={id}
              value={value}
              checked={level === value}
              onChange={() => setLevel(value)}
            />
            {gameResultText(
              locale,
              value === 1 ? 'single' : value === 2 ? 'gammon' : 'backgammon',
            )}
          </label>
        ))}
      </fieldset>
      <div role="status" aria-live="polite" className="play-resign-award">
        <strong>{`${formatNumber(level)} × ${gameResultText(locale, 'cube')} ${formatNumber(cube)} = ${formatNumber(points)}`}</strong>
        <p>
          {t('award', {
            player: opponent,
            points: `${formatNumber(points)} ${gameResultText(locale, points === 1 ? 'point' : 'points')}`,
          })}
        </p>
        <p className="play-resign-score">
          <span>{t(matchLength ? 'score' : 'moneyScore')}:</span>
          <strong>{`${white} ${formatNumber(projectedScores[0])} : ${formatNumber(projectedScores[1])} ${black}`}</strong>
          <span>
            {matchLength
              ? t('target', { points: matchLength })
              : appT('common.moneyGame')}
          </span>
        </p>
        <p className="play-resign-outcome" data-match-ending={endsMatch}>
          <strong>
            {t(
              endsMatch
                ? 'matchEnds'
                : matchLength
                  ? 'matchContinues'
                  : 'sessionContinues',
              {
                player: opponent,
              },
            )}
          </strong>
        </p>
      </div>
      <div className="play-actions play-cube-choices">
        <ActionButton
          type="button"
          icon="cancel"
          label={t('cancel')}
          disabled={busy}
          onClick={onClose}
        />
        {/* A resignation that ends the match already carries this label. */}
        {onForfeit && !endsMatch && (
          <ActionButton
            type="button"
            icon="resign"
            label={t('forfeitMatch')}
            disabled={busy}
            aria-haspopup="dialog"
            onClick={onForfeit}
          />
        )}
        <ActionButton
          type="button"
          icon="resign"
          label={t(
            endsMatch && matchLength > 0 ? 'confirmMatch' : 'confirmGame',
          )}
          className="play-resign-confirm"
          disabled={busy}
          onClick={() => onConfirm(level)}
        />
      </div>
    </PlayBoardDialog>
  );
}
