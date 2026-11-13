import type { ReactNode } from 'react';
import { DecisionDetails, DecisionFeedbackHeading } from './PlayFeedback';
import PlayMovePreview, {
  type PlayMovePreviewContext,
} from './PlayMovePreview';
import type { PlaySession } from './play-api';
import { usePlayModeText } from './play-mode-messages';

export default function PlayHintView({
  session,
  children,
  ...preview
}: { session: PlaySession; children?: ReactNode } & PlayMovePreviewContext) {
  const t = usePlayModeText();
  return (
    <PlayMovePreview session={session} {...preview}>
      {(onPreview, selectedPreview) => (
        <section
          className="play-feedback"
          data-testid={children ? undefined : 'play-feedback'}
          aria-label={t('feedback')}
        >
          <DecisionFeedbackHeading session={session} />
          {session.pending_decision?.revealed && (
            <DecisionDetails
              decision={session.pending_decision}
              session={session}
              onPreview={onPreview}
              selectedPreview={selectedPreview}
            />
          )}
          {children}
        </section>
      )}
    </PlayMovePreview>
  );
}
