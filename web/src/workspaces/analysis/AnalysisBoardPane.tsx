import type { RefObject } from 'react';
import ActionButton from '../../shared/ui/ActionButton';
import Board, { type Theme } from '../../shared/board/Board';
import { displayedBoardPosition } from '../../shared/board/displayed-board';
import type { BoardView, MovePreview } from '../../shared/board/move-preview';
import { useBoardReviewText } from '../../shared/board/board-review-messages';
import type { PreviewCandidate } from '../../shared/review/candidate-preview';
import type { Match, Position } from '../../shared/http/api';
import { useI18n } from '../../shared/i18n/i18n';
import PositionBookmarkButton from '../positions/PositionBookmarkButton';
import BoardReviewControls from './BoardReviewControls';
import { BoardDecisionComparison } from './BoardDecisionReview';
import { useSourceText } from './candidates/analysis-source-messages';
import GamePlanPanel from './game-plans/GamePlanPanel';
import MatchErrorNavigation from './match/MatchErrorNavigation';
import type { MatchReview } from './match/useMatchReview';
import type { MoveNotesTarget } from './notes/MoveNotesDialog';
import type { ReviewDecision } from './review-decisions-api';
import type { AnalysisCatalog } from './useAnalysisCatalog';

export default function AnalysisBoardPane({
  match,
  position,
  boardPosition,
  previewPosition,
  preview,
  effectiveView,
  selectedPreview,
  cube,
  cubeBoard,
  cubeMoveId,
  theme,
  active,
  loading,
  index,
  positionsCount,
  gamePositionCount,
  navigationError,
  showErrorNavigation,
  matchReview,
  catalog,
  bookmarkRevision,
  currentMoveRef,
  moveFilterControlRef,
  onBoardView,
  onPreview,
  onBookmarkChanged,
  onOpenDatabase,
  onReturnFromCube,
  onMoveNotes,
  onPrevious,
  onNext,
  onResume,
}: {
  match: Match;
  position?: Position;
  boardPosition?: Position;
  previewPosition?: Position;
  preview?: MovePreview;
  effectiveView: BoardView;
  selectedPreview?: PreviewCandidate;
  cube?: ReviewDecision;
  cubeBoard?: Position | null;
  cubeMoveId?: string | null;
  theme: Theme;
  active: boolean;
  loading: boolean;
  index: number;
  positionsCount: number;
  gamePositionCount?: number;
  navigationError: unknown;
  showErrorNavigation: boolean;
  matchReview: MatchReview;
  catalog: AnalysisCatalog;
  bookmarkRevision: number;
  currentMoveRef: RefObject<HTMLButtonElement | null>;
  moveFilterControlRef: RefObject<HTMLSelectElement | null>;
  onBoardView: (view: BoardView) => void;
  onPreview: (candidate: PreviewCandidate | null, view?: BoardView) => void;
  onBookmarkChanged: () => void;
  onOpenDatabase: () => void;
  onReturnFromCube: () => void;
  onMoveNotes: (target?: Omit<MoveNotesTarget, 'match'>) => void;
  onPrevious: () => void;
  onNext: () => void;
  onResume: () => void;
}) {
  const { t, formatNumber, formatDecision } = useI18n();
  const review = useBoardReviewText();
  const sourceText = useSourceText();

  return (
    <section className="board-panel" id="analysis-board-panel">
      {showErrorNavigation && (
        <div>
          <MatchErrorNavigation
            review={matchReview}
            catalog={catalog}
            match={match}
          />
        </div>
      )}
      {boardPosition ? (
        <>
          <div className="board-heading">
            <span>
              {cube && cube.checker_no == null
                ? t('matchAnalysis.cubeMove', {
                    game: cube.game_no,
                    decision: cube.decision_no,
                  })
                : t('app.gameMove', {
                    game: boardPosition.game_no,
                    move: boardPosition.ply,
                  })}
            </span>
            <strong>
              {!cubeBoard && effectiveView === 'after'
                ? review('after')
                : t('app.toMove', {
                    player:
                      boardPosition.player === 0 ? match.white : match.black,
                  })}
            </strong>
          </div>
          {!cubeBoard && position && (
            <div className="board-review-toolbar">
              {preview && (
                <BoardReviewControls
                  view={effectiveView}
                  preview={preview}
                  position={previewPosition ?? position}
                  selected={selectedPreview}
                  onView={onBoardView}
                  onPlayed={() => {
                    onPreview(null);
                    (
                      currentMoveRef.current ?? moveFilterControlRef.current
                    )?.focus({ preventScroll: true });
                    currentMoveRef.current?.scrollIntoView({
                      block: 'nearest',
                    });
                  }}
                />
              )}
              <div className="board-position-actions">
                <PositionBookmarkButton
                  positionId={position.id}
                  revision={bookmarkRevision}
                  onChanged={onBookmarkChanged}
                />
                <ActionButton
                  icon="search"
                  label={t('app.prepareSearch')}
                  compact
                  title={t('app.prepareSearch')}
                  onClick={onOpenDatabase}
                />
              </div>
            </div>
          )}
          {cube && cubeBoard && (
            <BoardDecisionComparison
              played={cube.played}
              best={cube.best}
              loss={
                cube.evaluation_status === 'evaluated' ? cube.equity_loss : null
              }
              source={[cube.engine, cube.depth].filter(Boolean).join(' · ')}
              context={sourceText(
                /^(take|drop)$/i.test(cube.played)
                  ? 'cubeResponse'
                  : 'cubeBeforeRoll',
              )}
              onReturn={position ? onReturnFromCube : undefined}
              onDetails={
                cubeMoveId
                  ? () =>
                      onMoveNotes({
                        gameNo: cube.game_no,
                        moveId: cubeMoveId,
                        decisionKind: 'cube',
                        reviewDecision: cube,
                      })
                  : undefined
              }
            />
          )}
          <Board
            position={boardPosition}
            white={match.white}
            black={match.black}
            theme={theme}
            preview={cubeBoard ? undefined : preview}
            previewMode={cubeBoard ? 'before' : effectiveView}
            previewLabel={
              cubeBoard
                ? undefined
                : review('showing', {
                    view: review(effectiveView),
                    player:
                      boardPosition.player === 0 ? match.white : match.black,
                    move: formatDecision(
                      selectedPreview?.notation ?? boardPosition.notation,
                    ),
                  })
            }
          />
          <GamePlanPanel
            board={
              displayedBoardPosition(
                boardPosition,
                cubeBoard ? undefined : preview,
                effectiveView,
              ).board
            }
            white={match.white}
            black={match.black}
            active={active}
          />
          {position && !cubeBoard && (
            <div className="board-navigation">
              <ActionButton
                icon="back"
                label={t('app.previousMove')}
                compact
                disabled={index === 0 || loading}
                onClick={onPrevious}
              />
              <span>
                {t('common.played')}{' '}
                <strong>{formatDecision(position.notation, position)}</strong>
                <small>
                  {formatNumber(index + 1)} /{' '}
                  {formatNumber(gamePositionCount ?? positionsCount)}
                </small>
              </span>
              <ActionButton
                icon="next"
                label={t('app.nextMove')}
                compact
                disabled={loading || index >= positionsCount - 1}
                onClick={onNext}
              />
            </div>
          )}
          <div className="position-code">
            <span>{t('app.positionCode')}</span>
            <code>{boardPosition.position_id}</code>
          </div>
        </>
      ) : (
        <div className="board-loading" role="status">
          {loading ? t('app.positionsLoading') : t('app.noPositions')}
          {!!navigationError && (
            <ActionButton
              icon="refresh"
              label={t('games.retry')}
              compact
              onClick={onResume}
            />
          )}
        </div>
      )}
    </section>
  );
}
