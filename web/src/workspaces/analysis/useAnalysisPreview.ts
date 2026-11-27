import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useWorkspaceDefaults } from '../../settings/configuration/ConfigurationProvider';
import { useWorkspaceLayout } from '../../shared/hooks/useWorkspaceSession';
import { projectMove, type BoardView } from '../../shared/board/move-preview';
import type { PreviewCandidate } from '../../shared/review/candidate-preview';
import { useBoardReviewShortcuts } from '../../shared/review/useBoardReviewShortcuts';
import type { Match, Position } from '../../shared/http/api';
import type { useGameNavigation } from '../../app/useGameNavigation';
import type { CubeAction, GameCubeActions } from './useGameCubeActions';
import {
  canOpenReviewDecision,
  findCubeDecisionByNumber,
  findCubeReviewDecision,
  type ReviewDecision,
} from './review-decisions-api';
import type { AnalysisDetail } from './AnalysisWorkspace';

export function useAnalysisPreview({
  navigation,
  match,
  gameNo,
  position,
  analysisId,
  sourceAvailable,
  active,
  notesSuspended,
  errors,
  reviewingCheckers,
  cubeActions,
  moveListRef,
  requestedCubeDecision,
  onDetail,
  onPosition,
}: {
  navigation: ReturnType<typeof useGameNavigation>;
  match?: Match | null;
  gameNo?: number;
  position?: Position;
  analysisId?: string;
  sourceAvailable: boolean;
  active: boolean;
  notesSuspended: boolean;
  errors: boolean;
  reviewingCheckers: boolean;
  cubeActions?: GameCubeActions;
  moveListRef: React.RefObject<HTMLDivElement | null>;
  requestedCubeDecision?: {
    matchId: string;
    gameNo: number;
    decisionNo: number;
    token: number;
  };
  onDetail: (detail: AnalysisDetail) => void;
  onPosition: (id: string) => void;
}) {
  const legacy = useWorkspaceDefaults();
  const [boardView, setBoardView] = useWorkspaceLayout<BoardView>(
    'analysis.boardView',
    legacy?.appearance.board_view,
    'arrows',
    (value) =>
      value === 'before' || value === 'arrows' || value === 'after'
        ? value
        : undefined,
  );
  const [candidate, setCandidate] = useState<
    PreviewCandidate & {
      positionId: string;
      analysisId?: string;
      view?: BoardView;
    }
  >();
  const selectedPreview =
    candidate?.positionId === position?.id &&
    candidate?.analysisId === analysisId
      ? candidate
      : undefined;
  const onPreview = useCallback(
    (selected: PreviewCandidate | null, view?: BoardView) => {
      setCandidate(
        selected && position
          ? {
              ...selected,
              positionId: position.id,
              analysisId,
              view,
            }
          : undefined,
      );
    },
    [position, analysisId],
  );
  const onBoardView = useCallback(
    (view: BoardView) => {
      setBoardView(view);
      setCandidate((current) =>
        current?.view ? { ...current, view: undefined } : current,
      );
    },
    [setBoardView],
  );
  const previewDice = selectedPreview?.dice;
  // A Dice equity preview shows its own roll; everything else uses the recorded one.
  const previewPosition = useMemo(
    () =>
      position && previewDice
        ? { ...position, dice: [...previewDice] }
        : position,
    [position, previewDice],
  );
  const preview = useMemo(
    () =>
      previewPosition
        ? projectMove(previewPosition, selectedPreview?.notation)
        : undefined,
    [previewPosition, selectedPreview?.notation],
  );
  const effectiveView = preview?.ok
    ? (selectedPreview?.view ?? boardView)
    : 'before';
  useBoardReviewShortcuts(
    active && !!selectedPreview && !notesSuspended,
    '.candidate-preview-button, .dice-grid button',
    onBoardView,
    preview?.ok === true,
  );

  const intent = navigation.intent();
  const [reviewSelection, setReviewSelection] = useState<{
    row: ReviewDecision;
    matchId: string;
    intent: number;
    jobRevision?: string;
  }>();
  const reviewRow =
    reviewSelection?.matchId === match?.id &&
    reviewSelection?.intent === intent &&
    reviewSelection?.jobRevision === analysisId
      ? reviewSelection.row
      : undefined;
  const cube = errors && reviewRow?.kind === 'cube' ? reviewRow : undefined;
  const cubeBoard = cube?.review_position;
  const cubeMoveId = cube?.move_id;
  // Mark the reviewed cube action only while the history shows its line.
  const reviewedCubeMove =
    cubeBoard &&
    cubeMoveId &&
    cubeActions &&
    [...cubeActions.before.values(), cubeActions.trailing].some((actions) =>
      actions.some((action) => action.id === cubeMoveId),
    )
      ? cubeMoveId
      : undefined;
  useEffect(() => {
    if (reviewedCubeMove)
      moveListRef.current
        ?.querySelector('.move-history-cube.current')
        ?.scrollIntoView({ block: 'nearest' });
  }, [reviewedCubeMove, moveListRef]);

  const boardPosition = cubeBoard ?? previewPosition;
  const checkerReview =
    reviewingCheckers &&
    reviewRow?.kind === 'checker' &&
    reviewRow.position_uuid === position?.id
      ? reviewRow
      : undefined;

  function openReviewDecision(row: ReviewDecision) {
    if (!match) return;
    if (row.kind === 'cube' && row.review_position) {
      setCandidate(undefined);
      if (row.game_no !== gameNo) void navigation.selectGame(row.game_no);
      setReviewSelection({
        row,
        matchId: match.id,
        intent: navigation.intent(),
        jobRevision: analysisId,
      });
    } else if (row.kind === 'checker' && row.position_uuid) {
      onPreview(null);
      onPosition(row.position_uuid);
      setReviewSelection({
        row,
        matchId: match.id,
        intent: navigation.intent(),
        jobRevision: analysisId,
      });
    }
  }

  const cubeLookup = useRef<AbortController>(undefined);
  const [unratedCubeAction, setUnratedCubeAction] = useState<string>();
  useEffect(() => () => cubeLookup.current?.abort(), []);
  async function openCubeAction(action: CubeAction) {
    if (!match || gameNo === undefined) return;
    cubeLookup.current?.abort();
    const abort = new AbortController();
    cubeLookup.current = abort;
    const started = navigation.intent();
    setUnratedCubeAction(undefined);
    let row: ReviewDecision | undefined;
    try {
      row = analysisId
        ? await findCubeReviewDecision(match.id, gameNo, action.id, {
            analysisId,
            signal: abort.signal,
          })
        : undefined;
    } catch {
      row = undefined;
    }
    if (abort.signal.aborted || navigation.intent() !== started) return;
    if (row && canOpenReviewDecision(row)) {
      onDetail('errors');
      openReviewDecision(row);
    } else setUnratedCubeAction(action.id);
  }

  const handledCubeRequest = useRef(0);
  const cubeRequestReady =
    requestedCubeDecision &&
    match?.id === requestedCubeDecision.matchId &&
    gameNo === requestedCubeDecision.gameNo &&
    sourceAvailable
      ? analysisId
      : undefined;
  useEffect(() => {
    const request = requestedCubeDecision;
    if (
      !request ||
      !cubeRequestReady ||
      handledCubeRequest.current === request.token
    )
      return;
    const abort = new AbortController();
    const started = navigation.intent();
    void findCubeDecisionByNumber(
      request.matchId,
      request.gameNo,
      request.decisionNo,
      { analysisId: cubeRequestReady, signal: abort.signal },
    )
      .then((row) => {
        if (abort.signal.aborted || navigation.intent() !== started) return;
        handledCubeRequest.current = request.token;
        if (row && canOpenReviewDecision(row)) {
          onDetail('errors');
          openReviewDecision(row);
        }
      })
      .catch(() => undefined);
    return () => abort.abort();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestedCubeDecision?.token, cubeRequestReady]);

  return {
    intent,
    selectedPreview,
    onPreview,
    onBoardView,
    previewPosition,
    preview,
    effectiveView,
    cube,
    cubeBoard,
    cubeMoveId,
    reviewedCubeMove,
    unratedCubeAction,
    boardPosition,
    checkerReview,
    openReviewDecision,
    openCubeAction,
    clearReviewSelection: () => setReviewSelection(undefined),
  };
}
