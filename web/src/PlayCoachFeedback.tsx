import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type RefObject,
} from 'react';
import ActionButton from './ActionButton';
import PlayBoardDialog from './PlayBoardDialog';
import { DecisionEvaluation, useDecisionChoice } from './PlayFeedback';
import PlayHintView from './PlayHintView';
import {
  hasPlayMovePreview,
  type PlayMovePreviewContext,
} from './PlayMovePreview';
import type { PlayDecision, PlaySession } from './play-api';
import { usePlayModeText } from './play-mode-messages';

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
  const choice = useDecisionChoice(session);
  const panel = useRef<HTMLElement>(null);
  const detailsTrigger = useRef<HTMLButtonElement>(null);
  const [detailsOpen, setDetailsOpen] = useState(false);
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
          <p className="play-coach-choice">{choice}</p>
          {decision.revealed && <DecisionEvaluation decision={decision} />}
          <ActionButton
            className="primary play-main-action"
            disabled={busy}
            onClick={onContinue}
            icon="confirm"
            label={decision.action === 'move' ? t('playMove') : t('continue')}
          />
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
