import type { Theme } from '../../../shared/board/themes';
import StudyCardExercise from './StudyCardExercise';
import { useLearningText } from './learning-messages';
import type { FsrsReviewSession } from './useFsrsReviewSession';

export default function CardReviewView({
  deckId,
  deckName,
  theme,
  visible,
  review,
}: {
  deckId: string;
  deckName?: string;
  theme: Theme;
  visible: boolean;
  review: FsrsReviewSession;
}) {
  const t = useLearningText();
  const card = review.activeNext?.card;
  return (
    <>
      {review.loading && <p role="status">{t('loading')}</p>}
      {review.activeNext && !card && (
        <p role="status">
          {t(
            review.activeNext.counts.new > 0 &&
              review.activeNext.counts.new_available === 0
              ? 'dailyLimitReached'
              : 'noDue',
          )}
        </p>
      )}
      {card && (
        <StudyCardExercise
          key={`${deckId}:${review.queueRevision}:${card.id}:${card.version}`}
          card={card}
          name={deckName}
          theme={theme}
          activeAttempt={review.activeNext?.active_attempt}
          revealed={review.revealed}
          busy={review.busy || review.attemptBusy}
          visible={visible}
          onAttemptStart={review.beginAttempt}
          onAttemptBusy={review.setAttemptBusy}
          onConflict={review.reloadCard}
          onReveal={() => void review.reveal()}
          onGrade={(answer, rating) => void review.grade(answer, rating)}
        />
      )}
      <p className="learning-explainer learning-study-note">
        {t('separateProgress')} {t('retention')}
      </p>
    </>
  );
}
