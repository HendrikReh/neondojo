import type { BoardView, MovePreview } from './move-preview';
import type { PreviewCandidate } from './candidate-preview';
import { useBoardReviewText } from './board-review-messages';
import { useI18n } from './i18n';
import './board-review.css';

export default function BoardReviewControls({
  view,
  preview,
  selected,
  played,
  onView,
  onPlayed,
}: {
  view: BoardView;
  preview: MovePreview;
  selected?: PreviewCandidate;
  played: string;
  onView: (view: BoardView) => void;
  onPlayed: () => void;
}) {
  const l = useBoardReviewText(),
    { formatDecision } = useI18n();
  return (
    <div className="board-review-controls">
      <div className="board-view-buttons" role="group" aria-label={l('views')}>
        {(['before', 'arrows', 'after'] as const).map((mode) => (
          <button
            type="button"
            key={mode}
            aria-pressed={view === mode}
            disabled={mode !== 'before' && !preview.ok}
            onClick={() => onView(mode)}
          >
            {l(mode)}
          </button>
        ))}
      </div>
      <div className="board-review-selection">
        <button
          type="button"
          className="played-preview-button"
          aria-pressed={!selected}
          onClick={onPlayed}
        >
          {l('played')}
        </button>
        <strong>
          {selected
            ? l('candidate', {
                rank: selected.rank,
                move: formatDecision(selected.notation),
              })
            : formatDecision(played)}
        </strong>
        {selected && <small>{selected.source}</small>}
      </div>
      {!preview.ok ? (
        <p className="board-review-warning" role="status">
          {l('unavailable')}
        </p>
      ) : preview.forced ? (
        <p className="board-review-help">{l('pass')}</p>
      ) : view === 'arrows' ? (
        <p className="board-review-help">{l('arrowsHelp')}</p>
      ) : null}
      {view === 'after' && (
        <p className="board-review-help">{l('analysisTarget')}</p>
      )}
    </div>
  );
}
