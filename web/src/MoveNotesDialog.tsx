import { useEffect, useState } from 'react';
import type { Match } from './api';
import {
  loadDecisions,
  movePosition,
  type GameDecisions,
  type StoredMove,
} from './library-api';
import Board from './Board';
import AnnotationPanel from './AnnotationPanel';
import ImportedMoveAnalysis from './ImportedMoveAnalysis';
import AnalysisDecisionSummary from './AnalysisDecisionSummary';
import type { ReviewDecision } from './review-decisions-api';
import LibraryDialog from './LibraryDialog';
import { useLibraryText } from './library-messages';
import { useI18n } from './i18n';
import type { Theme } from './themes';
export interface MoveNotesTarget {
  match: Match;
  gameNo: number;
  moveId?: string;
  decisionKind?: 'checker' | 'cube';
  reviewDecision?: ReviewDecision;
}
export default function MoveNotesDialog({
  target,
  theme,
  onClose,
  onSaved,
}: {
  target: MoveNotesTarget;
  theme: Theme;
  onClose: () => void;
  onSaved: () => void;
}) {
  const l = useLibraryText(),
    { formatError, formatDecision } = useI18n();
  const [catalog, setCatalog] = useState<GameDecisions>(),
    [moveId, setMoveId] = useState(target.moveId),
    [error, setError] = useState<unknown>();
  useEffect(() => {
    const abort = new AbortController();
    void loadDecisions(target.match.id, target.gameNo, abort.signal)
      .then((value) => {
        if (abort.signal.aborted) return;
        setCatalog(value);
        setMoveId(target.moveId ?? value.items[0]?.id);
      })
      .catch((e) => {
        if (!abort.signal.aborted) setError(e);
      });
    return () => abort.abort();
  }, [target]);
  const move: StoredMove | undefined = catalog?.items.find(
    (m) => m.id === moveId,
  );
  const kind = move?.position_uuid ? 'position' : 'move';
  const reviewingTarget = move?.id === target.moveId;
  const decision = reviewingTarget ? target.reviewDecision : undefined;
  return (
    <LibraryDialog title={l('cubeNotes')} onClose={onClose}>
      <p>
        {target.match.white} — {target.match.black} · {l('game')}{' '}
        {target.gameNo}
      </p>
      {!!error && (
        <p role="alert" className="error">
          {formatError(error)}
        </p>
      )}
      {!catalog && !error && <p role="status">{l('loading')}</p>}
      {catalog && catalog.items.length > 0 && (
        <label className="move-notes-selector">
          {l('sourceMove')}
          <select
            aria-label={l('sourceMove')}
            value={moveId}
            onChange={(e) => setMoveId(e.target.value)}
          >
            {catalog.items.map((m) => (
              <option value={m.id} key={m.id}>
                {m.move_no}.{' '}
                {m.player === 0 ? target.match.white : target.match.black} ·{' '}
                {formatDecision(m.notation)}
              </option>
            ))}
          </select>
        </label>
      )}
      {move && (
        <div className="move-notes-content">
          <div>
            <Board
              position={decision?.review_position ?? movePosition(move)}
              white={target.match.white}
              black={target.match.black}
              theme={theme}
            />
            {decision?.source === 'native' ? (
              <AnalysisDecisionSummary
                decision={decision}
                source={[decision.engine, decision.depth]
                  .filter(Boolean)
                  .join(' · ')}
              />
            ) : (
              <>
                <p>{formatDecision(decision?.played ?? move.notation)}</p>
                {target.reviewDecision?.source !== 'native' && (
                  <ImportedMoveAnalysis
                    moveId={move.id}
                    players={target.match}
                    decisionKind={
                      reviewingTarget ? target.decisionKind : undefined
                    }
                  />
                )}
              </>
            )}
          </div>
          <AnnotationPanel
            key={move.id}
            kind={kind}
            id={move.position_uuid ?? move.id}
            onSaved={onSaved}
          />
        </div>
      )}
      {catalog && !catalog.items.length && (
        <AnnotationPanel kind="game" id={catalog.game_id} onSaved={onSaved} />
      )}
    </LibraryDialog>
  );
}
