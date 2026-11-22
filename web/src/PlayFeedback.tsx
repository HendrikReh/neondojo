import { severityLabels } from './review-statistics-messages';
import ActionButton from './ActionButton';
import { useEffect, useRef } from 'react';
import { playSeverity, type PlayDecision, type PlaySession } from './play-api';
import { usePlayModeText } from './play-mode-messages';
import { usePlayText } from './play-messages';
import { CubeContextLine } from './CubeDecisionDetails';
import MoveExplanation from './MoveExplanation';
import { useI18n } from './i18n';
import PlayMovePreview, {
  hasPlayMovePreview,
  type PlayMovePreviewContext,
} from './PlayMovePreview';
import { useBoardReviewText } from './board-review-messages';
import {
  focusCandidateRow,
  navigateCandidatePreview,
} from './candidate-preview';

export function useDecisionChoice(session: PlaySession) {
  const t = usePlayModeText();
  const { formatDecision } = useI18n();
  const decision = session.pending_decision;
  const action = decision?.action ?? decision?.original_action;
  const index = decision?.action
    ? decision.move_index
    : decision?.original_move_index;
  const notation = decision?.action
    ? decision.notation
    : decision?.original_notation;
  return action === 'roll'
    ? t('notDoubled')
    : action === 'double'
      ? t('doubled')
      : action
        ? formatDecision(
            notation ||
              (index != null ? session.legal_moves[index]?.notation : null) ||
              action,
          )
        : '';
}

export function DecisionFeedbackHeading({ session }: { session: PlaySession }) {
  const t = usePlayModeText();
  const choice = useDecisionChoice(session);
  return (
    <h3>
      {t('feedback')}
      {choice && `: ${choice}`}
    </h3>
  );
}

const equityPrecision = { minimumFractionDigits: 3, maximumFractionDigits: 3 };

export function DecisionEvaluation({ decision }: { decision: PlayDecision }) {
  const t = usePlayModeText();
  const { formatNumber, t: appT } = useI18n();
  if (decision.loss == null) return null;
  return (
    <p
      className="play-feedback-result"
      data-severity={decision.severity ?? 'none'}
    >
      <strong>{appT(severityLabels[playSeverity(decision.severity)])}</strong> ·{' '}
      {t('loss')}: {formatNumber(decision.loss, equityPrecision)}
    </p>
  );
}

export function DecisionDetails({
  decision,
  session,
  onPreview,
  selectedPreview,
}: {
  onPreview?: (index: number) => void;
  selectedPreview?: number | null;
  decision: PlayDecision;
  session: PlaySession;
}) {
  const t = usePlayModeText();
  const playT = usePlayText();
  const review = useBoardReviewText();
  const previewIndices = decision.alternatives.flatMap((alternative, index) =>
    alternative.action === 'move' ? [index] : [],
  );
  const { formatNumber, formatDecision } = useI18n();
  const number = (value: number) => formatNumber(value, equityPrecision);
  const full = session.play_mode !== 'coach';
  const choice = (
    action: string | null | undefined,
    index: number | null | undefined,
  ) =>
    formatDecision(
      index != null
        ? (session.legal_moves[index]?.notation ?? action ?? '')
        : action === 'roll'
          ? 'no double'
          : (action ?? ''),
    );
  const cubeDecision =
    decision.alternatives.length > 0 && previewIndices.length === 0;
  // The engine's top checker play and the play the feedback is about.
  const bestMove = decision.alternatives.find(
    (alternative) => alternative.action === 'move' && alternative.loss === 0,
  );
  const chosenMove =
    decision.action === 'move' ? (decision.notation ?? undefined) : undefined;
  const playerName = (player: number) =>
    player === session.human_player
      ? session.human_name || playT('you')
      : playT('bot');
  return (
    <>
      <DecisionEvaluation decision={decision} />
      {cubeDecision && (
        <CubeContextLine
          matchId={session.position.gnubg_match_id}
          players={[playerName(0), playerName(1)]}
        />
      )}
      {decision.original_action && (
        <p>
          {t('first')}:{' '}
          {decision.original_notation != null
            ? formatDecision(decision.original_notation)
            : choice(decision.original_action, decision.original_move_index)}
          {decision.first_loss != null && (
            <> · Δ {number(decision.first_loss)}</>
          )}
          {' · '}
          {t(
            (decision.attempts[0]?.assisted ?? decision.assisted)
              ? 'assisted'
              : 'independent',
          )}
        </p>
      )}
      {decision.alternatives.length > 0 && (
        <div className="play-feedback-results">
          <table>
            <caption>{t('alternatives')}</caption>
            <thead>
              <tr>
                <th>{t('selected')}</th>
                <th>Δ</th>
                {full && (
                  <>
                    <th>{t('equity')}</th>
                    <th>{t('probabilities')}</th>
                  </>
                )}
              </tr>
            </thead>
            <tbody
              onKeyDown={
                onPreview
                  ? (event) =>
                      navigateCandidatePreview(event, (index) =>
                        onPreview(previewIndices[index]),
                      )
                  : undefined
              }
            >
              {decision.alternatives.map((alternative, index) => (
                <tr
                  key={`${alternative.action}:${alternative.move_index}:${index}`}
                  className={
                    onPreview && alternative.action === 'move'
                      ? 'candidate-preview-row'
                      : undefined
                  }
                  data-preview-selected={
                    onPreview && alternative.action === 'move'
                      ? selectedPreview === index
                      : undefined
                  }
                  onClick={
                    onPreview && alternative.action === 'move'
                      ? (event) => {
                          focusCandidateRow(event.currentTarget);
                          onPreview(index);
                        }
                      : undefined
                  }
                >
                  <td>
                    {onPreview && alternative.action === 'move' ? (
                      <button
                        type="button"
                        className="candidate-preview-button"
                        aria-label={review('previewMove', {
                          rank: index + 1,
                          move: formatDecision(alternative.notation),
                        })}
                        aria-pressed={selectedPreview === index}
                      >
                        {formatDecision(alternative.notation)}
                      </button>
                    ) : (
                      formatDecision(alternative.notation)
                    )}
                  </td>
                  <td>{number(alternative.loss)}</td>
                  {full && (
                    <>
                      <td>
                        {alternative.equity == null
                          ? '—'
                          : number(alternative.equity)}
                      </td>
                      <td>
                        {alternative.probabilities && (
                          <>
                            {alternative.probabilities
                              .slice(0, 3)
                              .map(
                                (p) =>
                                  `${formatNumber(p * 100, { maximumFractionDigits: 1 })}%`,
                              )
                              .join(' / ')}
                            <br />
                            {t('opponent')}:{' '}
                            {alternative.probabilities
                              .slice(3, 6)
                              .map(
                                (p) =>
                                  `${formatNumber(p * 100, { maximumFractionDigits: 1 })}%`,
                              )
                              .join(' / ')}
                          </>
                        )}
                      </td>
                    </>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {bestMove && chosenMove && bestMove.notation !== chosenMove && (
        <MoveExplanation
          position={session.position}
          best={bestMove.notation}
          played={chosenMove}
          bestChances={bestMove.probabilities}
          playedChances={
            decision.alternatives.find(
              (alternative) =>
                alternative.action === 'move' &&
                alternative.notation === chosenMove,
            )?.probabilities
          }
        />
      )}
    </>
  );
}
export default function PlayFeedback({
  session,
  busy,
  onReveal,
  onRetry,
  onContinue,
  preview,
}: {
  session: PlaySession;
  busy: boolean;
  onReveal: () => void;
  onRetry: () => void;
  onContinue: () => void;
  preview?: PlayMovePreviewContext;
}) {
  const t = usePlayModeText();
  const decision = session.pending_decision;
  const panel = useRef<HTMLElement>(null);
  useEffect(() => {
    if (decision?.action && !busy) panel.current?.focus();
  }, [decision?.id, decision?.action, busy]);
  const quiet = session.play_mode === 'competition';
  if (!decision || (quiet && !decision.action)) return null;
  const tutorWarning = session.play_mode === 'tutor' && decision.warning;
  const actions = (
    <div className="play-actions">
      {!quiet && decision.action && (
        <ActionButton
          disabled={busy}
          onClick={onRetry}
          icon="reset"
          label={t('retry')}
        />
      )}
      {!quiet && (!decision.revealed || tutorWarning) && (
        <ActionButton
          disabled={busy || decision.revealed}
          onClick={onReveal}
          icon="solution"
          label={t('solution')}
        />
      )}
      {decision.action && (
        <ActionButton
          disabled={busy}
          onClick={onContinue}
          icon="confirm"
          label={t(!quiet && decision.warning ? 'anyway' : 'continue')}
          shortcut="Enter"
        />
      )}
    </div>
  );
  const renderPanel = (
    onPreview?: (index: number) => void,
    selectedPreview?: number | null,
  ) => (
    <section
      ref={panel}
      tabIndex={-1}
      className="play-feedback"
      data-testid="play-feedback"
      aria-label={t('feedback')}
      aria-busy={busy}
    >
      {quiet ? (
        <h3>{t('unconfirmed')}</h3>
      ) : (
        <DecisionFeedbackHeading session={session} />
      )}
      {!quiet && decision.warning && !decision.revealed && (
        <p role="status">{t('warning')}</p>
      )}
      {tutorWarning && actions}
      {!quiet && decision.revealed && (
        <DecisionDetails
          decision={decision}
          session={session}
          onPreview={onPreview}
          selectedPreview={selectedPreview}
        />
      )}
      {!tutorWarning && actions}
      {!quiet && decision.original_action && <small>{t('originalHelp')}</small>}
    </section>
  );
  return preview && hasPlayMovePreview(session) ? (
    <PlayMovePreview key={decision.id} session={session} {...preview}>
      {renderPanel}
    </PlayMovePreview>
  ) : (
    renderPanel()
  );
}
