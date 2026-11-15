import { useProfileValue } from './useProfileValue';
import ActionButton from './ActionButton';
import {
  useCallback,
  useMemo,
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type KeyboardEvent,
  type CSSProperties,
} from 'react';
import Board, { type Theme } from './Board';
import BoardReviewControls from './BoardReviewControls';
import { useBoardReviewShortcuts } from './useBoardReviewShortcuts';
import PositionBookmarkButton from './PositionBookmarkButton';
import { projectMove, type BoardView } from './move-preview';
import { useBoardReviewText } from './board-review-messages';
import type { PreviewCandidate, PreviewComparison } from './candidate-preview';
import { useStatisticsText } from './review-statistics-messages';
import { useSourceText } from './analysis-source-messages';
import {
  BoardCheckerComparison,
  BoardDecisionComparison,
} from './BoardDecisionReview';
import AnalysisPanel from './AnalysisPanel';
import AnalysisRunningJobs from './AnalysisRunningJobs';
import MatchAnalysisPanel, {
  type BoardReviewTarget,
} from './MatchAnalysisPanel';
import NotesPanel from './NotesPanel';
import GameSelector from './GameSelector';
import { AddCardsPanel } from './LearningPanel';
import { type Job } from './api';
import type { ReviewDecision } from './review-decisions-api';
import { useGameNavigation } from './useGameNavigation';
import { useJobs } from './useJobs';
import { useI18n } from './i18n';
import { useWorkspaceText } from './workspace-messages';
import type { AnnotationKind } from './annotation-api';
import { useMoveAssessments } from './useMoveAssessments';
import MoveHistoryRow from './MoveHistoryRow';
import GameResultSummary from './GameResultSummary';
import AnalysisMatchHeader from './AnalysisMatchHeader';
import { hasMatchSource } from './match-source';
import { useAnalysisMatchHeaderText } from './analysis-match-header-messages';
import './retained-position.css';
import GamePlanPanel from './GamePlanPanel';
import { displayedBoardPosition } from './displayed-board';
import { usePositionText } from './position-browser-messages';
import type { MoveNotesTarget } from './MoveNotesDialog';
import { useAnalysisBoardResize } from './useAnalysisBoardResize';
import type { MatchAnalysisEngine } from './useMatchAnalysis';
import { useAnalysisCatalog, type AnalysisCatalog } from './useAnalysisCatalog';
import AnalysisCatalogSelect from './AnalysisCatalogSelect';

export type AnalysisDetail =
  'overview' | 'statistics' | 'errors' | 'notes' | 'cards';
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
  runningJobs = [],
  theme,
  defaultEngine = 'gnubg',
  defaultPlies,
  analysisInstallationKey,
  sourceRevision,
  analysisRevision,
  metadataRevision,
  bookmarkRevision,
  onBookmarkChanged,
  configured,
  detail: requestedDetail,
  onDetail,
  onOpenDatabase,
  onFindSimilar,
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
  runningJobs?: Job[];
  theme: Theme;
  defaultEngine?: MatchAnalysisEngine;
  defaultPlies: number;
  analysisInstallationKey?: string;
  sourceRevision: number;
  analysisRevision?: string;
  metadataRevision?: number;
  bookmarkRevision: number;
  onBookmarkChanged: () => void;
  configured: boolean;
  detail: AnalysisDetail;
  onDetail: (detail: AnalysisDetail) => void;
  onOpenDatabase: () => void;
  onFindSimilar: () => void;
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
  const positionText = usePositionText();
  const { match, game, games, positions, position, index, loading, stats } =
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
  const [boardTarget, setBoardTarget] = useState<HTMLDivElement | null>(null);
  const overview = detail === 'overview';
  const analysisGrid = useRef<HTMLDivElement>(null);
  const boardResize = useAnalysisBoardResize(
    analysisGrid,
    active && !!match,
    overview,
  );
  const gridStyle: CSSProperties & { '--analysis-board-width': string } = {
    '--analysis-board-width': `${boardResize.width}px`,
  };
  const catalog = useAnalysisCatalog(
    match?.id,
    position?.id,
    JSON.stringify([
      sourceRevision,
      analysisRevision,
      jobs.jobs.analysis?.id,
      jobs.jobs.analysis?.status,
    ]),
  );
  const catalogRevision = JSON.stringify([
    analysisRevision,
    catalog.analysisId,
    catalog.selected?.job?.status,
    catalog.selected?.job?.completed_games,
    catalog.selected?.job?.updated_at,
  ]);
  const moveAssessments = useMoveAssessments({
    matchId: sourceAvailable && catalog.analysisId ? match?.id : undefined,
    gameNo: game?.game_no,
    analysisId: catalog.analysisId,
    analysisRevision: catalogRevision,
  });
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
  }, [position?.id, positions.length, detail, game?.result]);
  const [boardView, setBoardView] = useProfileValue<BoardView>(
    (c) => c.appearance.board_view,
    (c, board_view) => ({ ...c, appearance: { ...c.appearance, board_view } }),
    'arrows',
  );
  const assessment = position
    ? moveAssessments.assessments[position.id]
    : undefined;
  const [candidate, setCandidate] = useState<
    PreviewCandidate & { positionId: string; analysisId?: string }
  >();
  const selectedPreview =
    candidate?.positionId === position?.id &&
    candidate?.analysisId === catalog.analysisId
      ? candidate
      : undefined;
  const onPreview = useCallback(
    (selected: PreviewCandidate | null) => {
      setCandidate(
        selected && position
          ? {
              ...selected,
              positionId: position.id,
              analysisId: catalog.analysisId,
            }
          : undefined,
      );
    },
    [position, catalog.analysisId],
  );
  const preview = useMemo(
    () =>
      position ? projectMove(position, selectedPreview?.notation) : undefined,
    [position, selectedPreview?.notation],
  );
  const effectiveView = preview?.ok ? boardView : 'before';
  useBoardReviewShortcuts(
    active && !!selectedPreview && !notesSuspended,
    '.candidate-preview-button',
    setBoardView,
    preview?.ok === true,
  );

  const [comparison, setComparison] = useState<{
    positionId?: string;
    analysisId?: string;
    value: PreviewComparison;
  }>();
  const onComparison = useCallback(
    (value: PreviewComparison) => {
      setComparison({
        positionId: position?.id,
        analysisId: catalog.analysisId,
        value,
      });
    },
    [position?.id, catalog.analysisId],
  );
  const candidateComparison =
    comparison?.positionId === position?.id &&
    comparison?.analysisId === catalog.analysisId
      ? comparison?.value
      : undefined;

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
  const cube =
    (overview || detail === 'errors') && reviewRow?.kind === 'cube'
      ? reviewRow
      : undefined;
  const cubeBoard = cube?.review_position;
  const boardPosition = cubeBoard ?? position;
  function openReviewDecision(row: ReviewDecision) {
    if (!match) return;
    if (row.kind === 'cube' && row.review_position) {
      if (!overview && detail !== 'errors') onDetail('errors');
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
  const tabRefs = useRef<
    Partial<Record<AnalysisDetail, HTMLButtonElement | null>>
  >({});
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
      className={`analysis-workspace ${overview ? 'analysis-overview' : ''} ${sourceAvailable ? '' : 'analysis-retained'}`}
      aria-label={w('analysis')}
    >
      <AnalysisMatchHeader
        match={match}
        selectedAnalysis={catalog.selected}
        metadataRevision={metadataRevision}
        analysisRevision={analysisRevision}
        theme={theme}
        active={active && !notesSuspended}
      />
      <AnalysisCatalogSelect key={match.id} catalog={catalog} />
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
        {sourceAvailable && (
          <div className="workspace-analysis-command" ref={setCommandTarget} />
        )}
        <AnalysisRunningJobs
          jobs={runningJobs}
          displayedJobId={catalog.selected?.job?.id}
        />
      </div>
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
            {tab === 'statistics' ? statistics('tab') : w(tab)}
          </button>
        ))}
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
            <span>{formatNumber(game?.position_count ?? 0)}</span>
          </div>
          <div className="moves-list" ref={moveList}>
            {positions.map((p, i) => (
              <MoveHistoryRow
                key={p.id}
                position={p}
                players={[match.white, match.black]}
                assessment={moveAssessments.assessments[p.id]}
                current={i === index}
                buttonRef={i === index ? currentMove : undefined}
                onSelect={() => navigation.selectIndex(i)}
              />
            ))}
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
          {sourceAvailable && <div ref={setBoardTarget} />}
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
              {cube && cubeBoard ? (
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
                    cube.source === 'imported' && cube.move_id
                      ? () =>
                          onMoveNotes({
                            gameNo: cube.game_no,
                            moveId: cube.move_id!,
                            decisionKind: 'cube',
                            reviewPosition: cubeBoard,
                            reviewPlayed: cube.played,
                          })
                      : undefined
                  }
                />
              ) : position ? (
                <>
                  <BoardCheckerComparison
                    position={position}
                    assessment={assessment}
                    candidateComparison={candidateComparison}
                    reviewRow={
                      reviewRow?.kind === 'checker' &&
                      reviewRow.position_uuid === position.id
                        ? reviewRow
                        : undefined
                    }
                    selectedPreview={selectedPreview}
                    onPreview={onPreview}
                  />
                  <div className="board-review-toolbar">
                    {preview && (
                      <BoardReviewControls
                        view={effectiveView}
                        preview={preview}
                        selected={selectedPreview}
                        onView={setBoardView}
                        onPlayed={() => {
                          onPreview(null);
                          currentMove.current?.focus({ preventScroll: true });
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
                      <ActionButton
                        type="button"
                        icon="similar"
                        label={positionText('findSimilar')}
                        compact
                        onClick={onFindSimilar}
                      />
                    </div>
                  </div>
                </>
              ) : null}
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
                    <strong>{formatDecision(position.notation)}</strong>
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
            {!!jobs.error && (
              <p className="error" role="alert">
                {formatError(jobs.error)}
              </p>
            )}
            <AnalysisPanel
              jobs={jobs.jobs}
              catalog={catalog}
              restoring={jobs.restoring}
              active={active && overview && !cubeBoard}
              stats={sourceAvailable ? stats : null}
              gameNo={game?.game_no}
              defaultPlies={defaultPlies}
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
              sourceRevision={sourceRevision}
              analysisRevision={catalogRevision}
              onPreview={onPreview}
              selectedPreview={selectedPreview}
              onComparison={onComparison}
              disabled={!position || jobs.submitting || !configured}
              onAnalyze={(kind, plies) => void jobs.run(kind, plies)}
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
              onPosition={onPosition}
              onDecision={openReviewDecision}
              onTrackJob={onTrackJob}
              catalog={catalog}
              analysisRevision={catalogRevision}
              boardReview={
                boardTarget && active && !notesSuspended
                  ? {
                      element: boardTarget,
                      origin: {
                        game: game?.game_no,
                        ply: position?.ply,
                        row: reviewRow,
                      },
                      intent,
                      currentIntent: navigation.intent,
                      busy: loading,
                    }
                  : undefined
              }
              onShowErrors={() => onDetail('errors')}
              selectedDecisionId={reviewRow?.id}
              selectedPositionId={cubeBoard ? undefined : position?.id}
            />
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
  onPosition,
  onDecision,
  onTrackJob,
  catalog,
  boardReview,
  analysisRevision,
  onShowErrors,
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
  onPosition: (id: string) => void;
  onDecision: (decision: ReviewDecision) => void;
  onTrackJob: (job: Job | undefined) => void;
  catalog: AnalysisCatalog;
  boardReview?: BoardReviewTarget;
  analysisRevision?: string;
  onShowErrors: () => void;
  selectedPositionId?: string;
  selectedDecisionId?: string;
}) {
  return (
    <div
      className="workspace-match-results"
      hidden={
        detail !== 'overview' && detail !== 'errors' && detail !== 'statistics'
      }
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
        overview={detail === 'overview'}
        selectedPositionId={selectedPositionId}
        selectedDecisionId={selectedDecisionId}
        showResults={detail === 'overview' || detail === 'errors'}
        showStatistics={detail === 'statistics'}
        onShowErrors={onShowErrors}
        onGame={(number) => void navigation.selectGame(number)}
        catalog={catalog}
        onJob={onTrackJob}
        onAccepted={(job) => {
          onTrackJob(job);
          catalog.refresh();
        }}
        boardReview={boardReview}
        analysisRevision={analysisRevision}
        onPosition={onPosition}
      />
    </div>
  );
}
