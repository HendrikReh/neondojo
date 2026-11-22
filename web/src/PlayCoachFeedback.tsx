import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from 'react';
import ActionButton from './shared/ui/ActionButton';
import PlayBoardDialog from './shared/board/PlayBoardDialog';
import { DecisionEvaluation, useDecisionChoice } from './PlayFeedback';
import PlayHintView from './PlayHintView';
import {
  hasPlayMovePreview,
  type PlayMovePreviewContext,
} from './PlayMovePreview';
import { playSeverity, type PlayDecision, type PlaySession } from './play-api';
import { usePlayModeText } from './play-mode-messages';
import { usePlayText } from './play-messages';

export default function PlayCoachFeedback({
  anchor,
  session,
  decision,
  busy,
  errorNotice,
  onContinue,
  onRetry,
  onReveal,
  ...preview
}: PlayMovePreviewContext & {
  anchor: RefObject<HTMLDivElement | null>;
  session: PlaySession;
  decision: PlayDecision;
  busy: boolean;
  errorNotice: ReactNode;
  onContinue: () => void;
  onRetry: () => void;
  onReveal: () => void;
}) {
  const t = usePlayModeText();
  const playT = usePlayText();
  const choice = useDecisionChoice(session);
  const panel = useRef<HTMLElement>(null);
  const detailsTrigger = useRef<HTMLButtonElement>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);
  const checkerTurn = decision.action === 'move' || decision.action === 'pass';
  const severity =
    decision.revealed && decision.loss != null
      ? playSeverity(decision.severity)
      : 'none';
  useEffect(() => {
    panel.current?.focus({ preventScroll: true });
  }, []);

  return (
    <>
      <section
        className="play-board-toolbar play-coach-feedback"
        data-testid="play-board-toolbar"
        ref={panel}
        tabIndex={-1}
        aria-label={t('coach')}
      >
        <div
          className="play-coach-summary"
          data-testid="play-feedback"
          aria-busy={busy}
        >
          <h3>{t('coach')}</h3>
          <div className={`play-coach-note severity-${severity}`}>
            <p className="play-coach-choice">{choice}</p>
            {decision.revealed && <DecisionEvaluation decision={decision} />}
          </div>
          {/* Enter confirms here as in the regular turn toolbar; the dice
              pair remains a second way to continue a checker turn. */}
          {checkerTurn ? (
            <>
              <ActionButton
                className="primary play-main-action"
                data-play-action={
                  decision.action === 'pass' ? 'pass' : 'confirm'
                }
                disabled={busy}
                onClick={onContinue}
                icon={decision.action === 'pass' ? 'pass' : 'confirm'}
                label={playT(
                  decision.action === 'pass' ? 'passTurn' : 'confirm',
                )}
                shortcut="Enter"
              />
              <p>{playT(decision.action === 'pass' ? 'passReady' : 'ready')}</p>
            </>
          ) : (
            <ActionButton
              className="primary play-main-action"
              disabled={busy}
              onClick={onContinue}
              icon="confirm"
              label={t('continue')}
              shortcut="Enter"
            />
          )}
          <ActionButton
            ref={detailsTrigger}
            disabled={busy}
            aria-haspopup="dialog"
            onClick={() => setDetailsOpen(true)}
            icon="analysis"
            label={t('details')}
          />
        </div>
        {!detailsOpen && errorNotice}
      </section>
      {detailsOpen && (
        <PlayBoardDialog
          anchor={anchor}
          heading={t('details')}
          busy={busy}
          className={`play-analysis-dialog${hasPlayMovePreview(session) ? ' play-analysis-dialog--checker' : ''}`}
          onClose={() => setDetailsOpen(false)}
          returnFocus={detailsTrigger}
        >
          {errorNotice}
          <PlayHintView session={session} {...preview}>
            <div className="play-actions">
              {!decision.revealed && (
                <ActionButton
                  disabled={busy}
                  onClick={onReveal}
                  icon="solution"
                  label={t('solution')}
                />
              )}
              <ActionButton
                disabled={busy}
                onClick={onRetry}
                icon="reset"
                label={t('retry')}
              />
            </div>
            {decision.original_action && <small>{t('originalHelp')}</small>}
          </PlayHintView>
        </PlayBoardDialog>
      )}
    </>
  );
}
