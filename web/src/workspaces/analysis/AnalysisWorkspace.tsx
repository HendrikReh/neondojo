import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react';
import { useRovingTabs } from '../../shared/ui/TabList';
import type { Theme } from '../../shared/board/Board';
import { type Job, type Match } from '../../shared/http/api';
import type { useGameNavigation } from '../../app/useGameNavigation';
import type { useJobs } from '../../shared/jobs/useJobs';
import { useWorkspaceText } from '../../shared/i18n/workspace-messages';
import { useMoveAssessments } from '../../shared/review/useMoveAssessments';
import { useWorkspaceSession } from '../../shared/hooks/useWorkspaceSession';
import { useGameCubeActions } from './useGameCubeActions';
import { useMatchReview } from './match/useMatchReview';
import { usePositionCalculation } from './usePositionCalculation';
import type { DecisionKind } from './match/match-statistics-api';
import type { AnnotationKind } from './notes/annotation-api';
import type { MoveNotesTarget } from './notes/MoveNotesDialog';
import { hasMatchSource } from './match-source';
import { useAnalysisBoardResize } from './useAnalysisBoardResize';
import type { MatchAnalysisEngine } from './match/useMatchAnalysis';
import { useMoveListFollow } from './useMoveListFollow';
import { useAnalysisCatalog } from './useAnalysisCatalog';
import AnalysisHeaderBar from './AnalysisHeaderBar';
import AnalysisMoveHistoryPane, {
  parseMoveHistoryFilter,
  type MoveHistoryFilter,
} from './AnalysisMoveHistoryPane';
import AnalysisBoardPane from './AnalysisBoardPane';
import AnalysisInspectorPane from './AnalysisInspectorPane';
import { useAnalysisPreview } from './useAnalysisPreview';
import './retained-position.css';

export type AnalysisDetail =
  | 'overview'
  | 'statistics'
  | 'errors'
  | 'notes'
  | 'cards';

const detailTabs: AnalysisDetail[] = [
  'overview',
  'statistics',
  'errors',
  'notes',
  'cards',
];

export interface AnalysisRevisions {
  openingBook: number;
  analysis?: string;
  metadata?: number;
  bookmark: number;
  deck: number;
}

export interface AnalysisEngineConfig {
  defaultEngine?: MatchAnalysisEngine;
  defaultPlies: number;
  installationKey?: string;
  configured: boolean;
}

export interface AnalysisCallbacks {
  onDetail: (detail: AnalysisDetail) => void;
  onBookmarkChanged: () => void;
  onOpenDatabase: () => void;
  onAnalyzeMatch?: (match: Match) => void;
  onTrackJob: (job: Job | undefined) => void;
  onNotesSaved: () => void;
  onMoveNotes: (target?: Omit<MoveNotesTarget, 'match'>) => void;
  onDeckChanged: () => void;
  onPosition: (id: string) => void;
  onAnalysisRequestConsumed?: (job: Job) => void;
}

export interface AnalysisWorkspaceProps {
  navigation: ReturnType<typeof useGameNavigation>;
  jobs: ReturnType<typeof useJobs>;
  revisions: AnalysisRevisions;
  engine: AnalysisEngineConfig;
  callbacks: AnalysisCallbacks;
  theme: Theme;
  detail: AnalysisDetail;
  notesSuspended: boolean;
  welcome: ReactNode;
  active?: boolean;
  runningJobs?: Job[];
  requestedAnalysis?: Job;
  /** A cube decision to show once its game is open; each token applies once. */
  requestedCubeDecision?: {
    matchId: string;
    gameNo: number;
    decisionNo: number;
    token: number;
  };
  scopeRequest?: { kind: AnnotationKind; token: number };
}

export default function AnalysisWorkspace({
  navigation,
  jobs,
  revisions,
  engine,
  callbacks,
  theme,
  detail: requestedDetail,
  notesSuspended,
  welcome,
  active = true,
  runningJobs = [],
  requestedAnalysis,
  requestedCubeDecision,
  scopeRequest,
}: AnalysisWorkspaceProps) {
  const {
    defaultEngine = 'gnubg',
    defaultPlies,
    installationKey: analysisInstallationKey,
    configured,
  } = engine;
  const {
    onDetail,
    onBookmarkChanged,
    onOpenDatabase,
    onAnalyzeMatch,
    onTrackJob,
    onNotesSaved,
    onMoveNotes,
    onDeckChanged,
    onPosition,
    onAnalysisRequestConsumed,
  } = callbacks;
  const w = useWorkspaceText();
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
  const [checkerReviewMatch, setCheckerReviewMatch] = useState<string>();
  const overview = detail === 'overview';
  const errors = detail === 'errors';
  const reviewingCheckers =
    overview && sourceAvailable && !!match && checkerReviewMatch === match.id;
  const showErrorNavigation =
    (errors || (overview && sourceAvailable)) && active && !notesSuspended;

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
      revisions.analysis,
      jobs.jobs.analysis?.id,
      jobs.jobs.analysis?.status,
    ]),
  );
  const catalogRevision = JSON.stringify([
    revisions.analysis,
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestedAnalysis?.id, match?.id, catalog.saving]);

  const moveAssessments = useMoveAssessments({
    matchId: sourceAvailable && catalog.analysisId ? match?.id : undefined,
    gameNo: game?.game_no,
    analysisId: catalog.analysisId,
    analysisRevision: catalogRevision,
  });
  const [moveFilter, setMoveFilter] = useWorkspaceSession<MoveHistoryFilter>(
    'analysis.moveHistory',
    undefined,
    'all',
    parseMoveHistoryFilter,
  );
  const filteredMoves = sourceAvailable && moveFilter !== 'all';
  const cubeActions = useGameCubeActions(
    sourceAvailable && !filteredMoves ? match?.id : undefined,
    game?.game_no,
  );
  const history = useMemo(
    () =>
      positions.flatMap((move, posIndex) => {
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
        return [{ move, index: posIndex, assessment }];
      }),
    [positions, moveAssessments.assessments, filteredMoves, moveFilter],
  );
  const moveFilterControl = useRef<HTMLSelectElement>(null);
  const { list: moveList, current: currentMove } = useMoveListFollow(
    JSON.stringify([position?.id, moveFilter, detail]),
    [history, game?.result],
  );

  const previewState = useAnalysisPreview({
    navigation,
    match,
    gameNo: game?.game_no,
    position,
    analysisId: catalog.analysisId,
    sourceAvailable,
    active,
    notesSuspended,
    errors,
    reviewingCheckers,
    cubeActions,
    moveListRef: moveList,
    requestedCubeDecision,
    onDetail,
    onPosition,
  });

  const calculationActive = active && !previewState.cubeBoard;
  const calculation = usePositionCalculation({
    positionJobs: jobs,
    positionId: position?.id,
    engine: defaultEngine,
    defaultPlies,
    disabled: !position || !configured,
    active: calculationActive,
    open: !!match,
    onShowResults: () => onDetail('overview'),
    onResetPreview: () => previewState.onPreview(null),
  });
  const positionCalculation = calculationActive ? calculation : undefined;
  const detailRoving = useRovingTabs<AnalysisDetail>({
    items: availableTabs,
    active: detail,
    onSelect: onDetail,
  });
  function showReview(kind: DecisionKind) {
    if (kind === 'checker') setCheckerReviewMatch(match?.id);
    const destination = kind === 'cube' ? 'errors' : 'overview';
    onDetail(destination);
    requestAnimationFrame(() => detailRoving.focusTab(destination));
  }
  const matchReview = useMatchReview({
    match: match && sourceAvailable ? match : undefined,
    catalog,
    analysisRevision: catalogRevision,
    gameNo: game?.game_no,
    reviewKind: overview ? 'checker' : 'cube',
    showResults: errors,
    checkerFiltersOpen: reviewingCheckers,
    board: showErrorNavigation
      ? {
          origin: {
            game: game?.game_no,
            ply: position?.ply,
            row: previewState.cube ?? previewState.checkerReview,
          },
          intent: previewState.intent,
          currentIntent: navigation.intent,
          busy: loading,
        }
      : undefined,
    selectedPositionId: previewState.cubeBoard ? undefined : position?.id,
    selectedDecisionId: (previewState.cube ?? previewState.checkerReview)?.id,
    onDecision: previewState.openReviewDecision,
    onShowReview: showReview,
    onGame: (number) => void navigation.selectGame(number),
  });

  if (!match)
    return <section className="analysis-workspace">{welcome}</section>;
  return (
    <section
      className={`analysis-workspace analysis-${detail} ${sourceAvailable ? '' : 'analysis-retained'}`}
      aria-label={w('analysis')}
    >
      <AnalysisHeaderBar
        match={match}
        games={games}
        gameNo={game?.game_no}
        detail={detail}
        availableTabs={availableTabs}
        detailRoving={detailRoving}
        sourceAvailable={sourceAvailable}
        theme={theme}
        active={active}
        notesSuspended={notesSuspended}
        metadataRevision={revisions.metadata}
        analysisRevision={revisions.analysis}
        catalogRevision={catalogRevision}
        catalog={catalog}
        defaultEngine={defaultEngine}
        defaultPlies={defaultPlies}
        configured={configured}
        analysisInstallationKey={analysisInstallationKey}
        positionCalculation={positionCalculation}
        runningJobs={runningJobs}
        onSelectGame={(number) => void navigation.selectGame(number)}
        onDetail={onDetail}
        onRefreshPositionJobs={jobs.refresh}
        onTrackJob={onTrackJob}
        onAnalyzeMatch={onAnalyzeMatch}
      />
      <div
        className={`analysis-grid${boardResize.dragging ? ' analysis-resizing' : ''}`}
        ref={analysisGrid}
        style={gridStyle}
        id="inspector-panel-overview"
        role={overview ? 'tabpanel' : undefined}
        aria-labelledby={overview ? 'inspector-tab-overview' : undefined}
      >
        <AnalysisMoveHistoryPane
          match={match}
          game={game}
          positionsCount={positions.length}
          history={history}
          index={index}
          loading={loading}
          sourceAvailable={sourceAvailable}
          moveFilter={moveFilter}
          onMoveFilter={setMoveFilter}
          filteredMoves={filteredMoves}
          moveAssessments={moveAssessments}
          cubeActions={cubeActions}
          reviewedCubeMove={previewState.reviewedCubeMove}
          unratedCubeAction={previewState.unratedCubeAction}
          onOpenCubeAction={(action) =>
            void previewState.openCubeAction(action)
          }
          moveFilterControlRef={moveFilterControl}
          moveListRef={moveList}
          currentMoveRef={currentMove}
          inErrorsTab={errors}
          onDetail={onDetail}
          onSelectIndex={navigation.selectIndex}
        />
        <AnalysisBoardPane
          match={match}
          position={position}
          boardPosition={previewState.boardPosition}
          previewPosition={previewState.previewPosition}
          preview={previewState.preview}
          effectiveView={previewState.effectiveView}
          selectedPreview={previewState.selectedPreview}
          cube={previewState.cube}
          cubeBoard={previewState.cubeBoard}
          cubeMoveId={previewState.cubeMoveId}
          theme={theme}
          active={active}
          loading={loading}
          index={index}
          positionsCount={positions.length}
          gamePositionCount={game?.position_count}
          navigationError={navigation.error}
          showErrorNavigation={showErrorNavigation}
          matchReview={matchReview}
          catalog={catalog}
          bookmarkRevision={revisions.bookmark}
          currentMoveRef={currentMove}
          moveFilterControlRef={moveFilterControl}
          onBoardView={previewState.onBoardView}
          onPreview={previewState.onPreview}
          onBookmarkChanged={onBookmarkChanged}
          onOpenDatabase={onOpenDatabase}
          onReturnFromCube={previewState.clearReviewSelection}
          onMoveNotes={onMoveNotes}
          onPrevious={navigation.previous}
          onNext={navigation.next}
          onResume={() => void navigation.resume()}
        />
        {boardResize.handle}
        <AnalysisInspectorPane
          match={match}
          games={games}
          gameNo={game?.game_no}
          position={position}
          gameLoaded={gameLoaded}
          loading={loading}
          active={active}
          detail={detail}
          sourceAvailable={sourceAvailable}
          theme={theme}
          defaultPlies={defaultPlies}
          openingBookRevision={revisions.openingBook}
          deckRevision={revisions.deck}
          catalogRevision={catalogRevision}
          jobs={jobs}
          calculation={calculation}
          calculationActive={calculationActive}
          catalog={catalog}
          matchReview={matchReview}
          reviewingCheckers={reviewingCheckers}
          checkerReview={previewState.checkerReview}
          cube={previewState.cube}
          cubeBoard={previewState.cubeBoard}
          selectedPreview={previewState.selectedPreview}
          notesSuspended={notesSuspended}
          scopeRequest={scopeRequest}
          onCloseCheckerReview={() => {
            setCheckerReviewMatch(undefined);
            detailRoving.focusTab('overview');
          }}
          onPreview={previewState.onPreview}
          onMoveNotes={onMoveNotes}
          onNotesSaved={onNotesSaved}
          onDeckChanged={onDeckChanged}
        />
      </div>
    </section>
  );
}
