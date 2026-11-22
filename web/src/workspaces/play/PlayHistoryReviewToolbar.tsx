import ActionButton from '../../shared/ui/ActionButton';
import { useI18n } from '../../shared/i18n/i18n';
import { useBoardReviewText } from '../../shared/board/board-review-messages';
import type { BoardView } from '../../shared/board/move-preview';
import { usePlayText } from './play-messages';
import type { usePlayReview } from './usePlayReview';

/** The toolbar above the board while a past turn of the match is reviewed. */
export default function PlayHistoryReviewToolbar({
  review,
  index,
  view,
  onRetry,
}: {
  review: ReturnType<typeof usePlayReview>;
  /** The reviewed turn; the toolbar renders only while one is selected. */
  index: number;
  view: BoardView;
  onRetry: () => void;
}) {
  const t = usePlayText();
  const reviewText = useBoardReviewText();
  const { formatError, formatNumber } = useI18n();
  const historical = review.entry;
  return (
    <section className="play-history-review play-board-toolbar">
      <div className="play-history-review-heading">
        <strong>
          {t('reviewing')} {formatNumber(index + 1)}
          {historical
            ? ` · ${t('game')} ${formatNumber(historical.game_no)}`
            : ''}
        </strong>
        <ActionButton
          type="button"
          onClick={review.reset}
          className="primary"
          icon="resume"
          label={t('returnToGame')}
          shortcut="Escape"
        >
          {t('returnToGame')}
        </ActionButton>
      </div>
      {review.loading && <p role="status">{t('loadingHistory')}</p>}
      {review.error !== undefined && (
        <div role="alert" className="play-error">
          <p>{t('historyError')}</p>
          <p>{formatError(review.error)}</p>
          <ActionButton onClick={onRetry} icon="refresh" label={t('retry')}>
            {t('retry')}
          </ActionButton>
        </div>
      )}
      {historical && (
        <div
          className="board-view-buttons text-navigation text-navigation--compact"
          role="group"
          aria-label={reviewText('views')}
        >
          {(['before', 'arrows', 'after'] as BoardView[])
            .filter(
              (mode) => historical.kind === 'checker' || mode !== 'arrows',
            )
            .map((mode) => (
              <button
                type="button"
                key={mode}
                aria-pressed={view === mode}
                aria-keyshortcuts={
                  mode === 'before'
                    ? 'ArrowLeft'
                    : mode === 'after'
                      ? 'ArrowRight'
                      : undefined
                }
                onClick={() => review.setView(mode)}
              >
                {reviewText(mode)}
              </button>
            ))}
        </div>
      )}
    </section>
  );
}
