import ActionButton from './shared/ui/ActionButton';
import { useMemo, useState, type ReactNode } from 'react';
import Board from './shared/board/Board';
import type { PlaySession } from './play-api';
import type { Theme } from './shared/board/themes';
import { projectMove, type BoardView } from './shared/board/move-preview';
import { useBoardReviewText } from './shared/board/board-review-messages';
import { usePlayText } from './play-messages';

export interface PlayMovePreviewContext {
  theme: Theme;
  white: string;
  black: string;
}
export function hasPlayMovePreview(session: PlaySession) {
  return (
    (session.play_mode ?? 'competition') !== 'competition' &&
    Boolean(
      session.pending_decision?.revealed &&
      session.pending_decision.alternatives.some((a) => a.action === 'move'),
    )
  );
}

/** Suggestions are projected from the saved turn, independently of unconfirmed live steps. */
export default function PlayMovePreview({
  session,
  theme,
  white,
  black,
  children,
}: {
  session: PlaySession;
  theme: Theme;
  white: string;
  black: string;
  children: (
    onPreview: ((index: number) => void) | undefined,
    selected: number | null,
  ) => ReactNode;
}) {
  const decision = session.pending_decision;
  const alternatives = hasPlayMovePreview(session)
    ? decision!.alternatives
    : [];
  const [selected, setSelected] = useState<number | null>(() => {
    const first = alternatives.findIndex((a) => a.action === 'move');
    return first < 0 ? null : first;
  });
  const [view, setView] = useState<BoardView>('arrows');
  const review = useBoardReviewText(),
    t = usePlayText();
  const candidate = selected === null ? null : alternatives[selected];
  const projection = useMemo(
    () =>
      candidate?.action === 'move'
        ? projectMove(session.position, candidate.notation)
        : undefined,
    [session.position, candidate],
  );
  const checkerHints = alternatives.some((a) => a.action === 'move');
  return (
    <div
      className={checkerHints ? 'play-decision-layout' : 'play-decision-text'}
    >
      {checkerHints && (
        <section
          className="play-decision-preview"
          aria-label={t('hintPreview')}
        >
          <div
            className="board-view-buttons text-navigation text-navigation--compact"
            role="group"
            aria-label={review('views')}
          >
            {(['before', 'arrows', 'after'] as BoardView[]).map((mode) => (
              <button
                type="button"
                key={mode}
                aria-pressed={view === mode}
                disabled={mode !== 'before' && !projection?.ok}
                onClick={() => setView(mode)}
              >
                {review(mode)}
              </button>
            ))}
            <ActionButton
              type="button"
              disabled={selected === null}
              onClick={() => {
                setSelected(null);
                setView('before');
              }}
              icon="reset"
              label={review('reset')}
              compact
            ></ActionButton>
          </div>
          <div
            className="play-board play-decision-board"
            data-testid="move-preview-board"
          >
            <Board
              position={session.position}
              theme={theme}
              white={white}
              black={black}
              preview={projection}
              previewMode={view}
              previewLabel={t('hintPreview')}
            />
          </div>
          {projection && !projection.ok && (
            <p role="status">{review('unavailable')}</p>
          )}
          <p className="play-mode-help">{t('hintPreviewHelp')}</p>
        </section>
      )}
      {children(
        checkerHints
          ? (index) => {
              setSelected(index);
              setView('arrows');
            }
          : undefined,
        selected,
      )}
    </div>
  );
}
