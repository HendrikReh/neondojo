import { useWorkspaceDefaults } from './settings/configuration/ConfigurationProvider';
import ActionButton from './shared/ui/ActionButton';
import {
  Fragment,
  useCallback,
  useMemo,
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type KeyboardEvent,
  type CSSProperties,
} from 'react';
import { useGameCubeActions, type CubeAction } from './useGameCubeActions';
import Board, { type Theme } from './shared/board/Board';
import BoardReviewControls from './BoardReviewControls';
import { useBoardReviewShortcuts } from './shared/review/useBoardReviewShortcuts';
import PositionBookmarkButton from './PositionBookmarkButton';
import { projectMove, type BoardView } from './shared/board/move-preview';
import { useBoardReviewText } from './shared/board/board-review-messages';
import type { PreviewCandidate } from './shared/review/candidate-preview';
import {
  severityLabels,
  useStatisticsText,
} from './shared/review/review-statistics-messages';
import { useSourceText } from './analysis-source-messages';
import { BoardDecisionComparison } from './BoardDecisionReview';
import AnalysisPanel from './AnalysisPanel';
import CubeReviewEvaluation from './CubeReviewEvaluation';
import AnalysisRunningJobs from './AnalysisRunningJobs';
import MatchAnalysisPanel, {
  type BoardReviewTarget,
} from './MatchAnalysisPanel';
import NotesPanel from './NotesPanel';
import GameSelector from './GameSelector';
import { AddCardsPanel } from './LearningPanel';
import { type Job, type Match } from './shared/api/api';
import {
  canOpenReviewDecision,
  findCubeDecisionByNumber,
  findCubeReviewDecision,
  type ReviewDecision,
} from './review-decisions-api';
import { type DecisionKind } from './match-statistics-api';
import { useGameNavigation } from './app/useGameNavigation';
import { useJobs } from './shared/jobs/useJobs';
import { useI18n } from './shared/i18n/i18n';
import { useWorkspaceText } from './shared/i18n/workspace-messages';
import type { AnnotationKind } from './annotation-api';
import { useMoveAssessments } from './shared/review/useMoveAssessments';
import { errorGrades } from './shared/review/move-severity';
import MoveHistoryRow from './MoveHistoryRow';
import GameResultSummary from './GameResultSummary';
import AnalysisMatchHeader from './AnalysisMatchHeader';
import { hasMatchSource } from './match-source';
import { useAnalysisMatchHeaderText } from './analysis-match-header-messages';
import './retained-position.css';
import GamePlanPanel from './GamePlanPanel';
import { displayedBoardPosition } from './shared/board/displayed-board';
import type { MoveNotesTarget } from './MoveNotesDialog';
import { useAnalysisBoardResize } from './useAnalysisBoardResize';
import type { MatchAnalysisEngine } from './useMatchAnalysis';
import { useWorkspaceLayout, useWorkspaceSession } from './shared/hooks/useWorkspaceSession';
import { useAnalysisCatalog, type AnalysisCatalog } from './useAnalysisCatalog';
import AnalysisCatalogSelect from './AnalysisCatalogSelect';

export type AnalysisDetail =
  'overview' | 'statistics' | 'errors' | 'notes' | 'cards';
const moveHistoryFilters = ['all', 'errors', ...errorGrades] as const;
const parseMoveHistoryFilter = (value: unknown) =>
  moveHistoryFilters.find((filter) => filter === value);
const detailTabs: AnalysisDetail[] = [
  'overview',
  'statistics',
  'errors',
  'notes',
  'cards',
];
export default function AnalysisWorkspace({
  navigation,
  jobs,
  requestedAnalysis,
  onAnalysisRequestConsumed,
  requestedCubeDecision,
  runningJobs = [],
  theme,
  defaultEngine = 'gnubg',
  defaultPlies,
  analysisInstallationKey,
  openingBookRevision,
  analysisRevision,
  metadataRevision,
  bookmarkRevision,
  onBookmarkChanged,
  configured,
  detail: requestedDetail,
  onDetail,
  onOpenDatabase,
  onAnalyzeMatch,
  onTrackJob,
  onNotesSaved,
  onMoveNotes,
  notesSuspended,
  deckRevision,
  onDeckChanged,
  onPosition,
  scopeRequest,
  welcome,
  active = true,
}: {
  navigation: ReturnType<typeof useGameNavigation>;
  jobs: ReturnType<typeof useJobs>;
  requestedAnalysis?: Job;
  onAnalysisRequestConsumed?: (job: Job) => void;
  /** A cube decision to show once its game is open; each token applies once. */
  requestedCubeDecision?: {
    matchId: string;
    gameNo: number;
    decisionNo: number;
    token: number;
  };
  runningJobs?: Job[];
  theme: Theme;
  defaultEngine?: MatchAnalysisEngine;
  defaultPlies: number;
  analysisInstallationKey?: string;
  openingBookRevision: number;
  analysisRevision?: string;
  metadataRevision?: number;
  bookmarkRevision: number;
  onBookmarkChanged: () => void;
  configured: boolean;
  detail: AnalysisDetail;
  onDetail: (detail: AnalysisDetail) => void;
  onOpenDatabase: () => void;
  onAnalyzeMatch?: (match: Match) => void;
  onTrackJob: (job: Job | undefined) => void;
  onNotesSaved: () => void;
  onMoveNotes: (target?: Omit<MoveNotesTarget, 'match'>) => void;
  notesSuspended: boolean;
  deckRevision: number;
  onDeckChanged: () => void;
  onPosition: (id: string) => void;
  scopeRequest?: { kind: AnnotationKind; token: number };
  welcome: ReactNode;
  active?: boolean;
}) {
  const { t, formatNumber, formatDecision, formatError } = useI18n();
  const w = useWorkspaceText();
  const sourceText = useSourceText();
  const review = useBoardReviewText();
  const statistics = useStatisticsText();
  const headerText = useAnalysisMatchHeaderText();
  const { match, game, games, positions, position, index, loading, gameLoaded } =
    navigation;
  const sourceAvailable = hasMatchSource(match);
  const detail =
    !sourceAvailable &&
    (requestedDetail === 'statistics' || requestedDetail === 'errors')
      ? 'overview'
      : requestedDetail;
  const availableTabs = sourceAvailable
    ? detailTabs
    : detailTabs.filter((tab) => tab !== 'statistics' && tab !== 'errors');
  const [commandTarget, setCommandTarget] = useState<HTMLDivElement | null>(
    null,
  );
  const [positionControls, setPositionControls] =
    useState<HTMLDivElement | null>(null);
  const [boardTarget, setBoardTarget] = useState<HTMLDivElement | null>(null);
  const [checkerControls, setCheckerControls] = useState<HTMLDivElement | null>(
    null,
  );
  const [checkerReviewMatch, setCheckerReviewMatch] = useState<string>();
  const overview = detail === 'overview';
  const errors = detail === 'errors';
  const reviewingCheckers =
    overview && sourceAvailable && !!match && checkerReviewMatch === match.id;
  const showErrorNavigation =
    (errors || reviewingCheckers) && active && !notesSuspended;
  const analysisGrid = useRef<HTMLDivElement>(null);
  const boardResize = useAnalysisBoardResize(
    analysisGrid,
    active && !!match,
    detail,
  );
  const gridStyle: CSSProperties & {
    '--analysis-board-width': string;
    '--analysis-viewport-space'?: string;
  } = {
    '--analysis-board-width': `${boardResize.width}px`,
    '--analysis-viewport-space':
      boardResize.viewportSpace === undefined
        ? undefined
        : `${boardResize.viewportSpace}px`,
  };
  const catalog = useAnalysisCatalog(
    match?.id,
    position?.id,
    JSON.stringify([
      analysisRevision,
      jobs.jobs.analysis?.id,
      jobs.jobs.analysis?.status,
    ]),
  );
  const catalogRevision = JSON.stringify([
    analysisRevision,
    catalog.analysisId,
    catalog.sourceRevision,
    catalog.selected?.job?.status,
    catalog.selected?.job?.completed_games,
    catalog.selected?.job?.updated_at,
  ]);
  useEffect(() => {
    if (
      requestedAnalysis?.match_uuid === match?.id &&
      requestedAnalysis &&
      !catalog.saving
    ) {
      void catalog.select(`job:${requestedAnalysis.id}`);
      onAnalysisRequestConsumed?.(requestedAnalysis);
    }
  }, [requestedAnalysis?.id, match?.id, catalog.saving]);
  const moveAssessments = useMoveAssessments({
    matchId: sourceAvailable && catalog.analysisId ? match?.id : undefined,
    gameNo: game?.game_no,
    analysisId: catalog.analysisId,
    analysisRevision: catalogRevision,
  });
  const [moveFilter, setMoveFilter] = useWorkspaceSession<
    (typeof moveHistoryFilters)[number]
  >('analysis.moveHistory', undefined, 'all', parseMoveHistoryFilter);
  const filteredMoves = sourceAvailable && moveFilter !== 'all';
  // Doubles, takes and passes are shown between checker moves when the
  // history is unfiltered, so a row never appears next to the wrong action.
  const cubeActions = useGameCubeActions(
    sourceAvailable && !filteredMoves ? match?.id : undefined,
    game?.game_no,
  );
  const cubeLookup = useRef<AbortController>(undefined);
  const [unratedCubeAction, setUnratedCubeAction] = useState<string>();
  useEffect(() => () => cubeLookup.current?.abort(), []);
  // A cube line opens its saved review; the lookup yields to newer navigation.
  async function openCubeAction(action: CubeAction) {
    if (!match || !game) return;
    cubeLookup.current?.abort();
    const abort = new AbortController();
    cubeLookup.current = abort;
    const started = navigation.intent();
    setUnratedCubeAction(undefined);
    const analysisId = catalog.analysisId;
    let row: ReviewDecision | undefined;
    try {
      row = analysisId
        ? await findCubeReviewDecision(match.id, game.game_no, action.id, {
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
  const cubeLines = (actions?: CubeAction[]) =>
    actions?.map((action) => (
      <button
        type="button"
        key={action.id}
        className={`move-history-cube${action.id === reviewedCubeMove ? ' current' : ''}`}
        aria-current={action.id === reviewedCubeMove ? 'true' : undefined}
        data-cube-action
        onClick={() => void openCubeAction(action)}
      >
        <i
          aria-hidden="true"
          className={`player-dot ${action.player === 0 ? 'white' : 'black'}`}
        />
        <span>
          {action.player === 0 ? match?.white : match?.black}:{' '}
          {formatDecision(action.notation)}
        </span>
        {unratedCubeAction === action.id && (
          <small role="status">{t('matchAnalysis.unavailable')}</small>
        )}
      </button>
    ));
  const history = useMemo(
    () =>
      positions.flatMap((move, index) => {
        const assessment = moveAssessments.assessments[move.id];
        if (
          filteredMoves &&
          (!assessment ||
            (moveFilter === 'errors'
              ? assessment.severity === 'none'
              : assessment.severity !== moveFilter))
        ) {
          return [];
        }
        return [{ move, index, assessment }];
      }),
    [positions, moveAssessments.assessments, filteredMoves, moveFilter],
  );
  const moveFilterControl = useRef<HTMLSelectElement>(null);
  const moveList = useRef<HTMLDivElement>(null);
  const currentMove = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const list = moveList.current,
      current = currentMove.current;
    if (!list || !current || !list.clientHeight) return;
    const outer = list.getBoundingClientRect();
    const result = current.nextElementSibling;
    const row = current.getBoundingClientRect();
    const bottom = result?.matches('.game-result-summary')
      ? result.getBoundingClientRect().bottom
      : row.bottom;
    if (row.top < outer.top) list.scrollTop -= outer.top - row.top;
    else if (bottom > outer.bottom) list.scrollTop += bottom - outer.bottom;
  }, [position?.id, history, detail, game?.result]);
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
    candidate?.analysisId === catalog.analysisId
      ? candidate
      : undefined;
  const onPreview = useCallback(
    (selected: PreviewCandidate | null, view?: BoardView) => {
      setCandidate(
        selected && position
          ? {
              ...selected,
              positionId: position.id,
              analysisId: catalog.analysisId,
              view,
            }
          : undefined,
      );
    },
    [position, catalog.analysisId],
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
  const jobRevision = catalog.analysisId;
  const reviewRow =
    reviewSelection?.matchId === match?.id &&
    reviewSelection?.intent === intent &&
    reviewSelection?.jobRevision === jobRevision
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
      moveList.current
        ?.querySelector('.move-history-cube.current')
        ?.scrollIntoView({ block: 'nearest' });
  }, [reviewedCubeMove]);
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
      if (row.game_no !== game?.game_no)
        void navigation.selectGame(row.game_no);
      setReviewSelection({
        row,
        matchId: match.id,
        intent: navigation.intent(),
        jobRevision,
      });
    } else if (row.kind === 'checker' && row.position_uuid) {
      onPreview(null);
      onPosition(row.position_uuid);
      setReviewSelection({
        row,
        matchId: match.id,
        intent: navigation.intent(),
        jobRevision,
      });
    }
  }
  // Open a cube decision requested from the decision search once its match,
  // game and displayed analysis are loaded. A handled token is not repeated,
  // and newer navigation discards a lookup that is still running.
  const handledCubeRequest = useRef(0);
  const cubeRequestReady =
    requestedCubeDecision &&
    match?.id === requestedCubeDecision.matchId &&
    game?.game_no === requestedCubeDecision.gameNo &&
    sourceAvailable
      ? catalog.analysisId
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
    // The request token and the loaded analysis identify one lookup.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestedCubeDecision?.token, cubeRequestReady]);
  const tabRefs = useRef<
    Partial<Record<AnalysisDetail, HTMLButtonElement | null>>
  >({});
  function showReview(kind: DecisionKind) {
    if (kind === 'checker') setCheckerReviewMatch(match?.id);
    const destination = kind === 'cube' ? 'errors' : 'overview';
    onDetail(destination);
    requestAnimationFrame(() => tabRefs.current[destination]?.focus());
  }
  function tabKey(
    event: KeyboardEvent<HTMLButtonElement>,
    current: AnalysisDetail,
  ) {
    let index = availableTabs.indexOf(current);
    if (event.key === 'ArrowRight') index = (index + 1) % availableTabs.length;
    else if (event.key === 'ArrowLeft')
      index = (index + availableTabs.length - 1) % availableTabs.length;
    else if (event.key === 'Home') index = 0;
    else if (event.key === 'End') index = availableTabs.length - 1;
    else return;
    event.preventDefault();
    onDetail(availableTabs[index]);
    tabRefs.current[availableTabs[index]]?.focus();
  }
  if (!match)
    return <section className="analysis-workspace">{welcome}</section>;
  return (
    <section
      className={`analysis-workspace analysis-${detail} ${sourceAvailable ? '' : 'analysis-retained'}`}
      aria-label={w('analysis')}
    >
      <div className="analysis-header-row">
        <AnalysisMatchHeader
          match={match}
          selectedAnalysis={catalog.selected}
          metadataRevision={metadataRevision}
          analysisRevision={analysisRevision}
          theme={theme}
          active={active && !notesSuspended}
        />
        <AnalysisCatalogSelect
          key={match.id}
          catalog={catalog}
          onDeleted={(entry) => {
            if (entry.scope === 'position') jobs.refresh();
          }}
        />
      </div>
      {!sourceAvailable && (
        <p className="analysis-retained-context" role="status">
          {headerText('retainedPosition')}
        </p>
      )}
      <div className="analysis-commandbar">
        <GameSelector
          games={games}
          selected={game?.game_no}
          onSelect={(number) => void navigation.selectGame(number)}
        />
        <div
          className="inspector-tabs analysis-detail-navigation text-navigation text-navigation--compact"
          role="tablist"
          aria-label={w('positionDetails')}
        >
          {availableTabs.map((tab) => (
            <button
              key={tab}
              role="tab"
              id={`inspector-tab-${tab}`}
              aria-controls={`inspector-panel-${tab}`}
              aria-selected={detail === tab}
              tabIndex={detail === tab ? 0 : -1}
              ref={(element) => {
                tabRefs.current[tab] = element;
              }}
              onKeyDown={(event) => tabKey(event, tab)}
              onClick={() => onDetail(tab)}
            >
              {tab === 'overview'
                ? t('common.checker')
                : tab === 'errors'
                  ? t('common.cube')
                  : tab === 'statistics'
                    ? statistics('tab')
                    : w(tab)}
            </button>
          ))}
        </div>
        {sourceAvailable && (
          <div className="workspace-analysis-command" ref={setCommandTarget} />
        )}
        {!sourceAvailable && (
          <div
            className="position-analysis-actions"
            ref={setPositionControls}
          />
        )}
        <AnalysisRunningJobs
          jobs={runningJobs}
          displayedJobId={catalog.selected?.job?.id}
        />
      </div>
      <div
        className={`analysis-grid${boardResize.dragging ? ' analysis-resizing' : ''}`}
        ref={analysisGrid}
        style={gridStyle}
        id="inspector-panel-overview"
        role={overview ? 'tabpanel' : undefined}
        aria-labelledby={overview ? 'inspector-tab-overview' : undefined}
      >
        <aside className="moves-panel">
          <div className="section-heading">
            <h3>{t('app.moveHistory')}</h3>
            <span>
              {filteredMoves && `${formatNumber(history.length)} / `}
              {formatNumber(game?.position_count ?? 0)}
            </span>
          </div>
          {sourceAvailable && (
            <select
              className="move-history-filter"
              ref={moveFilterControl}
              aria-label={t('moveHistory.filter')}
              value={moveFilter}
              disabled={!game?.position_count}
              onChange={(event) => {
                const value = event.target.value;
                setMoveFilter(parseMoveHistoryFilter(value) ?? 'all');
              }}
            >
              <option value="all">{t('moveHistory.all')}</option>
              <option value="errors">{t('moveHistory.errors')}</option>
              {errorGrades.map((grade) => (
                <option key={grade} value={grade}>
                  {t(severityLabels[grade])}
                </option>
              ))}
            </select>
          )}
          <div className="moves-list" ref={moveList}>
            {history.map(({ move, index: moveIndex, assessment }) => (
              <Fragment key={move.id}>
                {cubeLines(cubeActions?.before.get(move.id))}
                <MoveHistoryRow
                  position={move}
                  players={[match.white, match.black]}
                  assessment={assessment}
                  current={moveIndex === index && !reviewedCubeMove}
                  buttonRef={moveIndex === index ? currentMove : undefined}
                  onSelect={() => {
                    // A checker move has no cube review; show its candidates.
                    if (errors) onDetail('overview');
                    navigation.selectIndex(moveIndex);
                  }}
                />
              </Fragment>
            ))}
            {!filteredMoves && cubeLines(cubeActions?.trailing)}
            {filteredMoves &&
              history.length === 0 &&
              !moveAssessments.error && (
                <p className="move-history-status" role="status">
                  {loading || moveAssessments.loading
                    ? t('moveHistory.loading')
                    : positions.length === 0
                      ? t('app.noPositions')
                      : Object.keys(moveAssessments.assessments).length === 0
                        ? t('moveHistory.unrated')
                        : t('moveHistory.empty')}
                </p>
              )}
            {!loading && sourceAvailable && game?.result && (
              <GameResultSummary
                result={game.result}
                white={match.white}
                black={match.black}
              />
            )}
          </div>
          {!!moveAssessments.error && (
            <p className="move-assessment-error" role="alert">
              {formatError(moveAssessments.error)}
            </p>
          )}
        </aside>
        <section className="board-panel" id="analysis-board-panel">
          {showErrorNavigation && <div ref={setBoardTarget} />}
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
                          boardPosition.player === 0
                            ? match.white
                            : match.black,
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
                          currentMove.current ?? moveFilterControl.current
                        )?.focus({
                          preventScroll: true,
                        });
                        currentMove.current?.scrollIntoView({
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
                    cube.evaluation_status === 'evaluated'
                      ? cube.equity_loss
                      : null
                  }
                  source={[cube.engine, cube.depth].filter(Boolean).join(' · ')}
                  context={sourceText(
                    /^(take|drop)$/i.test(cube.played)
                      ? 'cubeResponse'
                      : 'cubeBeforeRoll',
                  )}
                  onReturn={
                    position ? () => setReviewSelection(undefined) : undefined
                  }
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
                          boardPosition.player === 0
                            ? match.white
                            : match.black,
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
                    onClick={navigation.previous}
                  />
                  <span>
                    {t('common.played')}{' '}
                    <strong>
                      {formatDecision(position.notation, position)}
                    </strong>
                    <small>
                      {formatNumber(index + 1)} /{' '}
                      {formatNumber(game?.position_count ?? positions.length)}
                    </small>
                  </span>
                  <ActionButton
                    icon="next"
                    label={t('app.nextMove')}
                    compact
                    disabled={loading || index >= positions.length - 1}
                    onClick={navigation.next}
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
              {!!navigation.error && (
                <ActionButton
                  icon="refresh"
                  label={t('games.retry')}
                  compact
                  onClick={() => void navigation.resume()}
                />
              )}
            </div>
          )}
        </section>
        {boardResize.handle}
        <aside className="position-inspector">
          <div
            className="overview-evaluation"
            role="region"
            aria-label={w('evaluation')}
            hidden={!overview || !!cubeBoard}
          >
            {reviewingCheckers && (
              <div className="checker-statistics-review">
                <div ref={setCheckerControls} />
                <ActionButton
                  type="button"
                  icon="clear"
                  label={statistics('closeCheckerReview')}
                  onClick={() => {
                    setCheckerReviewMatch(undefined);
                    tabRefs.current.overview?.focus();
                  }}
                />
                {checkerReview && (
                  <BoardDecisionComparison
                    played={checkerReview.played}
                    best={checkerReview.best}
                    context={[
                      checkerReview.evaluation_status === 'evaluated'
                        ? t(severityLabels[checkerReview.severity])
                        : undefined,
                      sourceText(
                        checkerReview.source === 'imported'
                          ? 'imported'
                          : 'engine',
                      ),
                    ]
                      .filter(Boolean)
                      .join(' · ')}
                    loss={
                      checkerReview.evaluation_status === 'evaluated'
                        ? checkerReview.equity_loss
                        : null
                    }
                    source={[checkerReview.engine, checkerReview.depth]
                      .filter(Boolean)
                      .join(' · ')}
                  />
                )}
              </div>
            )}
            {!!jobs.error && (
              <p className="error" role="alert">
                {formatError(jobs.error)}
              </p>
            )}
            <AnalysisPanel
              positionJobs={jobs}
              catalog={catalog}
              active={active && !cubeBoard}
              resultsVisible={active && overview && !cubeBoard && !loading}
              calculationTarget={positionControls}
              onShowResults={() => onDetail('overview')}
              observedMatchId={
                sourceAvailable && gameLoaded ? match.id : undefined
              }
              gameNo={game?.game_no}
              defaultPlies={defaultPlies}
              configuredEngine={defaultEngine}
              positionId={position?.id}
              position={position ?? undefined}
              players={match}
              playerName={
                position
                  ? position.player === 0
                    ? match.white
                    : match.black
                  : undefined
              }
              openingBookRevision={openingBookRevision}
              analysisRevision={catalogRevision}
              onPreview={onPreview}
              selectedPreview={selectedPreview}
              disabled={!position || !configured}
            />
          </div>
          {sourceAvailable && (
            <MatchResults
              matchId={match.id}
              gameNo={game?.game_no}
              navigation={navigation}
              defaultEngine={defaultEngine}
              defaultPlies={defaultPlies}
              configured={configured}
              analysisInstallationKey={analysisInstallationKey}
              detail={detail}
              target={commandTarget}
              positionControlsRef={setPositionControls}
              onDecision={openReviewDecision}
              onTrackJob={onTrackJob}
              onAnalyzeMatch={onAnalyzeMatch}
              catalog={catalog}
              analysisRevision={catalogRevision}
              boardReview={
                boardTarget && showErrorNavigation
                  ? {
                      element: boardTarget,
                      origin: {
                        game: game?.game_no,
                        ply: position?.ply,
                        row: cube ?? checkerReview,
                      },
                      intent,
                      currentIntent: navigation.intent,
                      busy: loading,
                    }
                  : undefined
              }
              onShowReview={showReview}
              checkerControls={reviewingCheckers ? checkerControls : null}
              selectedDecisionId={(cube ?? checkerReview)?.id}
              selectedPositionId={cubeBoard ? undefined : position?.id}
            />
          )}
          {cube && cubeBoard && (
            <div className="overview-evaluation">
              <CubeReviewEvaluation decision={cube} players={match} />
            </div>
          )}
          {errors && sourceAvailable && !cubeBoard && (
            <p className="analysis-empty cube-review-unselected">
              {review('cubeUnselected')}
            </p>
          )}
          <div
            id="inspector-panel-notes"
            role="tabpanel"
            aria-labelledby="inspector-tab-notes"
            hidden={detail !== 'notes'}
          >
            <NotesPanel
              key={match.id}
              matchId={match.id}
              gameNo={game?.game_no}
              onMoveNotes={onMoveNotes}
              suspended={notesSuspended}
              positionId={position?.id}
              onSaved={onNotesSaved}
              scopeRequest={scopeRequest}
            />
          </div>
          <div
            id="inspector-panel-cards"
            role="tabpanel"
            aria-labelledby="inspector-tab-cards"
            hidden={detail !== 'cards'}
          >
            <AddCardsPanel
              positionId={position?.id}
              matchId={match.id}
              analysisId={catalog.analysisId}
              gameNo={game?.game_no}
              allowBatch={sourceAvailable}
              white={match.white}
              black={match.black}
              theme={theme}
              revision={deckRevision}
              onChanged={onDeckChanged}
            />
          </div>
        </aside>
      </div>
    </section>
  );
}

// Keep the subscription mounted regardless of inspector tab or main workspace.
function MatchResults({
  navigation,
  defaultEngine,
  defaultPlies,
  analysisInstallationKey,
  configured,
  detail,
  target,
  positionControlsRef,
  onDecision,
  onTrackJob,
  onAnalyzeMatch,
  catalog,
  boardReview,
  analysisRevision,
  onShowReview,
  checkerControls,
  selectedPositionId,
  selectedDecisionId,
}: {
  matchId: string;
  gameNo?: number;
  navigation: ReturnType<typeof useGameNavigation>;
  defaultEngine: MatchAnalysisEngine;
  defaultPlies: number;
  analysisInstallationKey?: string;
  configured: boolean;
  detail: AnalysisDetail;
  target: HTMLElement | null;
  positionControlsRef: (element: HTMLDivElement | null) => void;
  onDecision: (decision: ReviewDecision) => void;
  onTrackJob: (job: Job | undefined) => void;
  onAnalyzeMatch?: (match: Match) => void;
  catalog: AnalysisCatalog;
  boardReview?: BoardReviewTarget;
  analysisRevision?: string;
  onShowReview: (kind: DecisionKind) => void;
  checkerControls: HTMLElement | null;
  selectedPositionId?: string;
  selectedDecisionId?: string;
}) {
  return (
    <div
      className="workspace-match-results"
      hidden={detail !== 'errors' && detail !== 'statistics'}
    >
      <MatchAnalysisPanel
        key={navigation.match!.id}
        match={navigation.match!}
        games={navigation.games}
        onDecision={onDecision}
        gameNo={navigation.game?.game_no}
        defaultEngine={defaultEngine}
        defaultPlies={defaultPlies}
        configured={configured}
        analysisInstallationKey={analysisInstallationKey}
        controlsTarget={target}
        positionControlsRef={positionControlsRef}
        selectedPositionId={selectedPositionId}
        selectedDecisionId={selectedDecisionId}
        showResults={detail === 'errors'}
        showStatistics={detail === 'statistics'}
        reviewKind={detail === 'overview' ? 'checker' : 'cube'}
        checkerControls={checkerControls}
        onShowReview={onShowReview}
        onGame={(number) => void navigation.selectGame(number)}
        catalog={catalog}
        onJob={onTrackJob}
        onAccepted={(job) => {
          onTrackJob(job);
          catalog.refresh();
        }}
        onAnalyzeMatch={
          onAnalyzeMatch && (() => onAnalyzeMatch(navigation.match!))
        }
        boardReview={boardReview}
        analysisRevision={analysisRevision}
      />
    </div>
  );
}
