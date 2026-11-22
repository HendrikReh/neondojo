import { useEffect, useMemo, useRef, useState } from 'react';
import Board from '../../../shared/board/Board';
import type { Position } from '../../../shared/api/api';
import type { Theme } from '../../../shared/board/themes';
import {
  projectMove,
  type BoardView,
} from '../../../shared/board/move-preview';
import { useBoardReviewText } from '../../../shared/board/board-review-messages';
import { useStudyText } from '../study-messages';
import { useI18n } from '../../../shared/i18n/i18n';
import ActionButton from '../../../shared/ui/ActionButton';
import type { ReviewDecisionEntry } from './learning-review-positions';
export default function LearningDecisionPreview({
  entry,
  position,
  theme,
  white,
  black,
  loading,
  error,
  onRetry,
  onBack,
  scrollRequest,
}: {
  entry: ReviewDecisionEntry;
  position?: Position;
  theme: Theme;
  white: string;
  black: string;
  loading: boolean;
  error?: unknown;
  onRetry: () => void;
  onBack: () => void;
  scrollRequest: number;
}) {
  const t = useStudyText(),
    review = useBoardReviewText();
  const { formatDecision, formatError } = useI18n();
  const [view, setView] = useState<BoardView>('arrows');
  const region = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!loading)
      region.current?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  }, [loading, position, error, scrollRequest]);
  const projection = useMemo(
    () =>
      position && entry.decision.kind === 'checker'
        ? projectMove(position, entry.decision.best)
        : undefined,
    [position, entry.decision],
  );
  return (
    <section
      ref={region}
      className="learning-decision-preview"
      aria-label={t('decisionPreview')}
    >
      <p className="learning-review-preview-label">
        <strong>
          {t('bestMove')}: {formatDecision(entry.decision.best)}
        </strong>{' '}
        · {entry.gameNo}.{entry.decision.decision_no}
      </p>
      {loading && <p role="status">{t('loadingPosition')}</p>}
      {!!error && (
        <div role="alert">
          <p>{formatError(error)}</p>
          <ActionButton icon="refresh" label={t('retry')} onClick={onRetry}>
            {t('retry')}
          </ActionButton>
        </div>
      )}
      {!loading && !error && position && (
        <>
          <div
            className="board-view-buttons text-navigation text-navigation--compact"
            role="group"
            aria-label={review('views')}
          >
            {(['before', 'arrows', 'after'] as BoardView[]).map((mode) => (
              <button
                key={mode}
                type="button"
                aria-pressed={(projection?.ok ? view : 'before') === mode}
                disabled={mode !== 'before' && !projection?.ok}
                onClick={() => setView(mode)}
              >
                {review(mode)}
              </button>
            ))}
          </div>
          <div
            className="play-board learning-review-preview-board"
            data-preview-decision={entry.key}
          >
            <Board
              position={position}
              theme={theme}
              white={white}
              black={black}
              preview={projection}
              previewMode={projection?.ok ? view : 'before'}
              previewLabel={t('bestMove')}
            />
          </div>
          {projection && !projection.ok && (
            <p role="status">{review('unavailable')}</p>
          )}
        </>
      )}
      {!loading && !error && !position && (
        <p role="status">{t('positionUnavailable')}</p>
      )}
      <ActionButton
        type="button"
        className="learning-preview-return"
        icon="next"
        label={t('decisions')}
        onClick={onBack}
      >
        {t('decisions')}
      </ActionButton>
    </section>
  );
}
