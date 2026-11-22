import type { RefObject } from 'react';
import type { Theme } from './shared/board/themes';
import PlayBoardDialog from './shared/board/PlayBoardDialog';
import PlayLearningReview from './PlayLearningReview';
import ActionButton from './shared/ui/ActionButton';
import type { PlaySession } from './play-api';
import { usePlayModeText } from './play-mode-messages';
import { useStudyText } from './study-messages';
import { useI18n } from './shared/i18n/i18n';
import './learning-review-dialog.css';
export default function LearningReviewDialog({
  anchor,
  returnFocus,
  session,
  theme,
  context,
  busy = false,
  error,
  onRetry,
  onClose,
}: {
  anchor?: RefObject<HTMLDivElement | null>;
  returnFocus: RefObject<HTMLElement | null>;
  session?: PlaySession;
  theme: Theme;
  context?: string;
  busy?: boolean;
  error?: unknown;
  onRetry?: () => void;
  onClose: () => void;
}) {
  const modeT = usePlayModeText(),
    t = useStudyText();
  const { formatError } = useI18n();
  return (
    <PlayBoardDialog
      anchor={anchor}
      returnFocus={returnFocus}
      heading={modeT('review')}
      busy={busy}
      className="learning-review-dialog"
      onClose={onClose}
    >
      {context && (
        <p className="learning-review-context" tabIndex={0}>
          {context}
        </p>
      )}
      {busy && <p role="status">{t('loadingReport')}</p>}
      {!!error && (
        <div role="alert">
          <p>{formatError(error)}</p>
          {onRetry && (
            <ActionButton icon="refresh" label={t('retry')} onClick={onRetry}>
              {t('retry')}
            </ActionButton>
          )}
        </div>
      )}
      {session?.review && (
        <PlayLearningReview
          key={`${session.id}:${session.version}`}
          session={session}
          theme={theme}
        />
      )}
    </PlayBoardDialog>
  );
}
