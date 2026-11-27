import { useState } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import LibraryDialog from '../../shared/ui/LibraryDialog';
import Tooltip from '../../shared/ui/Tooltip';
import EngineLabel from '../../shared/engine/EngineLabel';
import { formatPly } from '../../shared/format/ply';
import { useI18n } from '../../shared/i18n/i18n';
import type { Distribution, Job, Position } from '../../shared/http/api';
import {
  candidatePreviewNotation,
  type PreviewCandidate,
} from '../../shared/review/candidate-preview';
import { useLibraryText } from '../library/library-messages';
import type { PositionCalculation } from './usePositionCalculation';

/** Unordered roll key such as `3-6`; undefined unless both dice are valid. */
function rollKey(dice: readonly number[] | undefined) {
  if (
    dice?.length !== 2 ||
    dice.some((die) => !Number.isInteger(die) || die < 1 || die > 6)
  )
    return undefined;
  return `${Math.min(dice[0], dice[1])}-${Math.max(dice[0], dice[1])}`;
}

export default function DiceEquityTab({
  positionId,
  position,
  diceJob,
  distribution,
  restoring,
  submitting,
  pending,
  blocked,
  diceDeleted,
  hasJobError,
  calculation,
  removeDice,
  onPreview,
}: {
  positionId?: string;
  position?: Position;
  diceJob?: Job;
  distribution: Distribution | null;
  restoring: boolean;
  submitting: boolean;
  pending: boolean;
  blocked: boolean;
  diceDeleted: boolean;
  hasJobError: boolean;
  calculation: PositionCalculation;
  removeDice: (jobId: string) => Promise<void>;
  onPreview?: (candidate: PreviewCandidate | null) => void;
}) {
  const { t, formatPercent, formatEquity, formatDecision, formatError } =
    useI18n();
  const library = useLibraryText();
  const rolled = rollKey(position?.dice);
  const [rollChoice, setRollChoice] = useState<{
    positionId?: string;
    roll: string;
  }>();
  const selectedRoll =
    rollChoice && rollChoice.positionId === positionId
      ? rollChoice.roll
      : (rolled ?? '1-1');
  const [diceDeletion, setDiceDeletion] = useState<{
    positionId?: string;
    jobId: string;
    label: string;
    busy?: boolean;
    error?: unknown;
  }>();
  const deletionTarget =
    diceDeletion?.positionId === positionId ? diceDeletion : undefined;

  const confirmDiceDeletion = async () => {
    const target = deletionTarget;
    if (!target || target.busy) return;
    setDiceDeletion({ ...target, busy: true, error: undefined });
    calculation.setDeletedDice({
      positionId: target.positionId,
      engine: calculation.engine,
    });
    onPreview?.(null);
    try {
      await removeDice(target.jobId);
      setDiceDeletion(undefined);
    } catch (error) {
      calculation.setDeletedDice(undefined);
      setDiceDeletion((current) =>
        current?.jobId === target.jobId
          ? { ...target, busy: false, error }
          : current,
      );
    }
  };

  const best = distribution?.rolls.find(
    (r) => rollKey(r.dice) === selectedRoll,
  );
  const distributionLabel = distribution
    ? `${distribution.engine} · ${formatPly(distribution.plies)}`
    : '';

  return (
    <>
      {restoring || submitting ? (
        <p className="fineprint" role="status">
          {restoring ? library('loading') : t('matchAnalysis.submitting')}
        </p>
      ) : distribution ? (
        <div className="distribution">
          <div className="distribution-caption">
            <span>
              <EngineLabel details={distributionLabel} />
            </span>
            <span>{t('analysis.combinations')}</span>
            <strong>Ø {formatEquity(distribution.mean_equity)}</strong>
            <ActionButton
              icon="delete"
              label={t('analysis.deleteDice')}
              compact
              type="button"
              disabled={blocked || !diceJob}
              onClick={() =>
                diceJob &&
                setDiceDeletion({
                  positionId,
                  jobId: diceJob.id,
                  label: distributionLabel,
                })
              }
            />
          </div>
          <div className="dice-grid">
            {Array.from({ length: 36 }, (_, i) => {
              const a = Math.floor(i / 6) + 1,
                b = (i % 6) + 1;
              const key = `${Math.min(a, b)}-${Math.max(a, b)}`;
              const roll = distribution.rolls.find(
                (r) => rollKey(r.dice) === key,
              );
              const title = roll
                ? t('analysis.rollTitle', {
                    first: a,
                    second: b,
                    move: formatDecision(roll.best.notation),
                  })
                : undefined;
              return (
                <Tooltip
                  as="button"
                  key={i}
                  text={
                    title && key === rolled
                      ? `${title} · ${t('analysis.rolled')}`
                      : title
                  }
                  aria-pressed={selectedRoll === key}
                  className={`${roll && roll.best.equity >= distribution.mean_equity ? 'above' : 'below'}${selectedRoll === key ? ' selected' : ''}${key === rolled ? ' rolled' : ''}`}
                  onClick={() => {
                    setRollChoice({ positionId, roll: key });
                    onPreview?.(
                      roll
                        ? {
                            notation: candidatePreviewNotation(roll.best),
                            rank: roll.best.rank,
                            source: distributionLabel,
                            dice: [a, b],
                          }
                        : null,
                    );
                  }}
                >
                  <span>
                    {a} · {b}
                  </span>
                  <b>{roll ? formatEquity(roll.best.equity) : '—'}</b>
                </Tooltip>
              );
            })}
          </div>
          {best && (
            <p className="roll-detail">
              <strong>{best.dice.join('–')}</strong>
              <span>{formatDecision(best.best.notation)}</span>
              <b>{formatEquity(best.best.equity)}</b>
              {rollKey(best.dice) === rolled && (
                <span className="roll-detail-rolled">
                  {t('analysis.rolled')}
                </span>
              )}
              <small>
                {t('analysis.probability', {
                  value: formatPercent(best.probability, 2),
                })}
              </small>
            </p>
          )}
          <p className="fineprint">
            {t('analysis.distributionMethod', { plies: distribution.plies })}
            {rolled && ` ${t('analysis.rolledLegend')}`}
          </p>
        </div>
      ) : !pending && !hasJobError && !diceJob ? (
        <div className="panel-empty">
          <span className="empty-symbol">⚄</span>
          <h3>{t('analysis.diceEmptyHeading')}</h3>
          <p>{t('analysis.diceEmptyDescription')}</p>
          {diceDeleted && <p role="status">{t('analysis.diceDeleted')}</p>}
        </div>
      ) : null}
      {deletionTarget && (
        <LibraryDialog
          title={t('analysis.deleteDice')}
          onClose={() => setDiceDeletion(undefined)}
          busy={deletionTarget.busy}
          native
        >
          <p>{t('analysis.deleteDicePrompt')}</p>
          <p>
            <strong>{deletionTarget.label}</strong>
          </p>
          {!!deletionTarget.error && (
            <p className="error" role="alert">
              {formatError(deletionTarget.error)}
            </p>
          )}
          {deletionTarget.busy && (
            <p role="status">{t('analysis.deletingDice')}</p>
          )}
          <div className="library-dialog-actions">
            <ActionButton
              icon="cancel"
              label={library('cancel')}
              type="button"
              disabled={deletionTarget.busy}
              onClick={() => setDiceDeletion(undefined)}
            />
            <ActionButton
              icon="delete"
              className="danger"
              label={t('analysis.deleteDice')}
              type="button"
              disabled={deletionTarget.busy}
              onClick={() => void confirmDiceDeletion()}
            />
          </div>
        </LibraryDialog>
      )}
    </>
  );
}
