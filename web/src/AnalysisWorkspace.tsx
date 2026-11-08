import {
  useCallback,
  useMemo,
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type KeyboardEvent,
} from 'react';
import Board, { type Theme } from './Board';
import BoardReviewControls from './BoardReviewControls';
import { projectMove, type BoardView } from './move-preview';
import { useBoardReviewText } from './board-review-messages';
import type { PreviewCandidate } from './candidate-preview';
import { useStatisticsText } from './review-statistics-messages';
import AnalysisPanel from './AnalysisPanel';
import MatchAnalysisPanel from './MatchAnalysisPanel';
import NotesPanel from './NotesPanel';
import GameSelector from './GameSelector';
import { AddCardsPanel } from './LearningPanel';
import { request, type Job, type MoveAnalysisPage } from './api';
import { useGameNavigation } from './useGameNavigation';
import { useJobs } from './useJobs';
import { useI18n } from './i18n';
import { useWorkspaceText } from './workspace-messages';
import type { AnnotationKind } from './annotation-api';
import { useMoveAssessments } from './useMoveAssessments';
import { useSourceText } from './analysis-source-messages';

export type AnalysisDetail =
  'overview' | 'evaluation' | 'statistics' | 'errors' | 'notes' | 'cards';
const severityLabels = {
  none: 'severity.none',
  doubtful: 'severity.doubtful',
  bad: 'severity.bad',
  very_bad: 'severity.very_bad',
} as const;
const detailTabs: AnalysisDetail[] = [
  'overview',
  'evaluation',
  'statistics',
  'errors',
  'notes',
  'cards',
];
export default function AnalysisWorkspace({
  navigation,
  jobs,
  theme,
  defaultPlies,
  sourceRevision,
  analysisRevision,
  onBookSettings,
  configured,
  detail,
  onDetail,
  onOpenDatabase,
  onTrackJob,
  onNotesSaved,
  onMoveNotes,
  notesSuspended,
  deckRevision,
  onDeckChanged,
  onPosition,
  scopeRequest,
  welcome,
}: {
  navigation: ReturnType<typeof useGameNavigation>;
  jobs: ReturnType<typeof useJobs>;
  theme: Theme;
  defaultPlies: number;
  sourceRevision: number;
  analysisRevision?: string;
  onBookSettings: () => void;
  configured: boolean;
  detail: AnalysisDetail;
  onDetail: (detail: AnalysisDetail) => void;
  onOpenDatabase: () => void;
  onTrackJob: (job: Job | undefined) => void;
  onNotesSaved: () => void;
  onMoveNotes: () => void;
  notesSuspended: boolean;
  deckRevision: number;
  onDeckChanged: () => void;
  onPosition: (id: string) => void;
  scopeRequest?: { kind: AnnotationKind; token: number };
  welcome: ReactNode;
}) {
  const { t, formatNumber, formatEquity, formatDecision, formatError } =
    useI18n();
  const w = useWorkspaceText();
  const review = useBoardReviewText();
  const statistics = useStatisticsText();
  const sourceText = useSourceText();
  const { match, game, games, positions, position, index, loading, stats } =
    navigation;
  const [commandTarget, setCommandTarget] = useState<HTMLDivElement | null>(
    null,
  );
  const [focus, setFocus] = useState(false);
  const overview = detail === 'overview';
  const [matchJob, setMatchJob] = useState<Job>();
  const moveAssessments = useMoveAssessments({
    matchId: match?.id,
    gameNo: game?.game_no,
    job: matchJob,
    analysisRevision,
  });
  const moveList = useRef<HTMLDivElement>(null);
  const currentMove = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const list = moveList.current,
      current = currentMove.current;
    if (!list || !current || !list.clientHeight) return;
    const outer = list.getBoundingClientRect(),
      row = current.getBoundingClientRect();
    if (row.top < outer.top) list.scrollTop -= outer.top - row.top;
    else if (row.bottom > outer.bottom)
      list.scrollTop += row.bottom - outer.bottom;
  }, [position?.id, positions.length, detail, focus]);
  const [moveFilter, setMoveFilter] = useState<{
    player: string;
    kind: string;
    severity?: string;
    game_no?: number;
  }>({ player: '', kind: '' });
  const [boardView, setBoardView] = useState<BoardView>('arrows');
  const [candidate, setCandidate] = useState<
    PreviewCandidate & { positionId: string }
  >();
  const selectedPreview =
    candidate?.positionId === position?.id ? candidate : undefined;
  const onPreview = useCallback(
    (selected: PreviewCandidate | null) => {
      setCandidate(
        selected && position
          ? { ...selected, positionId: position.id }
          : undefined,
      );
      if (selected)
        setBoardView((view) => (view === 'before' ? 'arrows' : view));
    },
    [position],
  );
  const preview = useMemo(
    () =>
      position ? projectMove(position, selectedPreview?.notation) : undefined,
    [position, selectedPreview?.notation],
  );
  const effectiveView = preview?.ok ? boardView : 'before';
  const [nextErrorFailure, setNextErrorFailure] = useState<unknown>();
  const [noErrors, setNoErrors] = useState(false);
  const tabRefs = useRef<
    Partial<Record<AnalysisDetail, HTMLButtonElement | null>>
  >({});
  function tabKey(
    event: KeyboardEvent<HTMLButtonElement>,
    current: AnalysisDetail,
  ) {
    let index = detailTabs.indexOf(current);
    if (event.key === 'ArrowRight') index = (index + 1) % detailTabs.length;
    else if (event.key === 'ArrowLeft')
      index = (index + detailTabs.length - 1) % detailTabs.length;
    else if (event.key === 'Home') index = 0;
    else if (event.key === 'End') index = detailTabs.length - 1;
    else return;
    event.preventDefault();
    onDetail(detailTabs[index]);
    tabRefs.current[detailTabs[index]]?.focus();
  }
  const errorRequest = useRef<AbortController | undefined>(undefined);
  useEffect(() => {
    setNoErrors(false);
    setNextErrorFailure(undefined);
    return () => errorRequest.current?.abort();
  }, [
    match?.id,
    game?.game_no,
    position?.id,
    matchJob?.id,
    moveFilter.player,
    moveFilter.kind,
    moveFilter.severity,
    moveFilter.game_no,
  ]);
  async function nextError() {
    setNextErrorFailure(undefined);
    setNoErrors(false);
    if (!overview) onDetail('errors');
    if (
      !matchJob ||
      !match ||
      !game ||
      moveFilter.kind === 'cube' ||
      matchJob.match_uuid !== match.id ||
      (matchJob.game_no != null && matchJob.game_no !== game.game_no)
    ) {
      setNoErrors(true);
      return;
    }
    errorRequest.current?.abort();
    const abort = new AbortController();
    errorRequest.current = abort;
    const wantedGame = moveFilter.game_no ?? game.game_no;
    try {
      let after: number | null = null;
      let first: string | undefined;
      do {
        const page: MoveAnalysisPage = await request(
          `/jobs/${matchJob.id}/moves?errors_only=true&kind=checker${moveFilter.player ? `&player=${moveFilter.player}` : ''}${moveFilter.severity ? `&severity=${moveFilter.severity}` : ''}&game_no=${wantedGame}&limit=100${after ? `&after=${after}` : ''}`,
          { signal: abort.signal },
        );
        if (abort.signal.aborted) return;
        for (const row of page.items) {
          if (!row.position_uuid) continue;
          first ??= row.position_uuid;
          const target = positions.findIndex((p) => p.id === row.position_uuid);
          if (target > index) {
            onPosition(row.position_uuid);
            return;
          }
        }
        after = page.next_cursor ?? null;
      } while (after);
      if (first) onPosition(first);
      else setNoErrors(true);
    } catch (e) {
      if (!abort.signal.aborted) setNextErrorFailure(e);
    }
  }
  if (!match)
    return <section className="analysis-workspace">{welcome}</section>;
  return (
    <section
      className={`analysis-workspace ${overview ? 'analysis-overview' : ''} ${focus ? 'board-focus' : ''}`}
      aria-label={w('analysis')}
    >
      <div className="analysis-heading">
        <div>
          <h1>
            {match.white} <span>{t('common.versus')}</span> {match.black}
          </h1>
          <p>
            {match.match_length
              ? t('app.matchLength', { count: match.match_length })
              : t('common.moneyGame')}{' '}
            · {match.format.toUpperCase()}{' '}
            <a href={`/api/matches/${match.id}/source`}>SGF ↓</a>
          </p>
        </div>
      </div>
      <div className="analysis-commandbar">
        <GameSelector
          games={games}
          selected={game?.game_no}
          onSelect={(number) => void navigation.selectGame(number)}
        />
        <div className="workspace-analysis-command" ref={setCommandTarget} />
      </div>
      <div
        className="inspector-tabs analysis-detail-navigation"
        role="tablist"
        aria-label={w('positionDetails')}
      >
        {detailTabs.map((tab) => (
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
        className="analysis-grid"
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
            {positions.map((p, i) => {
              const assessment = moveAssessments.assessments[p.id];
              const description = assessment
                ? `${t(severityLabels[assessment.severity])} · Δ ${formatEquity(assessment.equity_loss)} · ${sourceText(assessment.source === 'imported' ? 'imported' : 'engine')} · ${assessment.engine}${assessment.depth ? ` · ${assessment.depth}` : ''}`
                : undefined;
              return (
                <button
                  key={p.id}
                  ref={i === index ? currentMove : undefined}
                  className={`${i === index ? 'current' : ''}${assessment ? ` move-assessment-${assessment.severity}` : ''}`}
                  data-position-id={p.id}
                  data-assessment-source={assessment?.source}
                  aria-current={i === index ? 'step' : undefined}
                  onClick={() => navigation.selectIndex(i)}
                >
                  <span className="move-no">
                    {p.game_no}.{p.ply}
                  </span>
                  <i
                    className={`player-dot ${p.player === 0 ? 'white' : 'black'}`}
                  />
                  <span className="move-dice">{p.dice.join('–')}</span>
                  <strong>{formatDecision(p.notation)}</strong>
                  {assessment && (
                    <span
                      className="move-assessment-marker"
                      title={description}
                    >
                      <span aria-hidden="true">
                        {assessment.severity === 'none'
                          ? '✓'
                          : assessment.severity === 'doubtful'
                            ? '?!'
                            : assessment.severity === 'bad'
                              ? '?'
                              : '??'}
                      </span>
                      <span className="sr-only">{description}</span>
                    </span>
                  )}
                </button>
              );
            })}
          </div>
          {!!moveAssessments.error && (
            <p className="move-assessment-error" role="alert">
              {formatError(moveAssessments.error)}
            </p>
          )}
          <button
            className="next-error"
            onClick={() => void nextError()}
            disabled={!matchJob || !game}
          >
            {w('nextError')} →
          </button>
          {noErrors && <p role="status">{w('noErrors')}</p>}
          {!!nextErrorFailure && (
            <p role="alert">{formatError(nextErrorFailure)}</p>
          )}
        </aside>
        <section className="board-panel">
          {position ? (
            <>
              <div className="board-heading">
                <span>
                  {t('app.gameMove', {
                    game: position.game_no,
                    move: position.ply,
                  })}
                </span>
                <strong>
                  {effectiveView === 'after'
                    ? review('after')
                    : t('app.toMove', {
                        player:
                          position.player === 0 ? match.white : match.black,
                      })}
                </strong>
              </div>
              {preview && (
                <BoardReviewControls
                  view={effectiveView}
                  preview={preview}
                  selected={selectedPreview}
                  onView={setBoardView}
                  onPlayed={() => onPreview(null)}
                />
              )}
              <Board
                position={position}
                white={match.white}
                black={match.black}
                theme={theme}
                preview={preview}
                previewMode={effectiveView}
                previewLabel={review('showing', {
                  view: review(effectiveView),
                  player: position.player === 0 ? match.white : match.black,
                  move: formatDecision(
                    selectedPreview?.notation ?? position.notation,
                  ),
                })}
              />
              <div className="board-navigation">
                <button
                  aria-label={t('app.previousMove')}
                  disabled={index === 0 || loading}
                  onClick={navigation.previous}
                >
                  ←
                </button>
                <span>
                  {t('common.played')}{' '}
                  <strong>{formatDecision(position.notation)}</strong>
                  <small>
                    {formatNumber(index + 1)} /{' '}
                    {formatNumber(game?.position_count ?? positions.length)}
                  </small>
                </span>
                <button
                  aria-label={t('app.nextMove')}
                  disabled={loading || index >= positions.length - 1}
                  onClick={navigation.next}
                >
                  →
                </button>
              </div>
              <div className="position-code">
                <span>{t('app.positionCode')}</span>
                <code>{position.position_id}</code>
                <button title={t('app.prepareSearch')} onClick={onOpenDatabase}>
                  ⌕
                </button>
              </div>
            </>
          ) : (
            <div className="board-loading" role="status">
              {loading ? t('app.positionsLoading') : t('app.noPositions')}
              {!!navigation.error && (
                <button onClick={() => void navigation.resume()}>
                  {t('games.retry')}
                </button>
              )}
            </div>
          )}
          <button
            className="board-focus-toggle"
            onClick={() => setFocus((value) => !value)}
          >
            {w(focus ? 'unfocus' : 'focus')}
          </button>
        </section>
        <aside className="position-inspector">
          <div
            id="inspector-panel-evaluation"
            className={overview ? 'overview-evaluation' : undefined}
            role={overview ? 'region' : 'tabpanel'}
            aria-labelledby="inspector-tab-evaluation"
            hidden={!overview && detail !== 'evaluation'}
          >
            {!!jobs.error && (
              <p className="error" role="alert">
                {formatError(jobs.error)}
              </p>
            )}
            <AnalysisPanel
              jobs={jobs.jobs}
              stats={stats}
              gameNo={game?.game_no}
              defaultPlies={defaultPlies}
              positionId={position?.id}
              sourceRevision={sourceRevision}
              onBookSettings={onBookSettings}
              onPreview={onPreview}
              selectedPreview={selectedPreview}
              disabled={!position || jobs.submitting || !configured}
              onAnalyze={(kind, plies) => void jobs.run(kind, plies)}
            />
          </div>
          <MatchResults
            matchId={match.id}
            gameNo={game?.game_no}
            navigation={navigation}
            defaultPlies={defaultPlies}
            configured={configured}
            detail={detail}
            target={commandTarget}
            onPosition={onPosition}
            onTrackJob={onTrackJob}
            onCurrentJob={setMatchJob}
            onFilterChange={setMoveFilter}
            onShowErrors={() => {
              if (!overview) onDetail('errors');
            }}
            selectedPositionId={position?.id}
          />
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
              gameNo={game?.game_no}
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
  defaultPlies,
  configured,
  detail,
  target,
  onPosition,
  onTrackJob,
  onCurrentJob,
  onFilterChange,
  onShowErrors,
  selectedPositionId,
}: {
  matchId: string;
  gameNo?: number;
  navigation: ReturnType<typeof useGameNavigation>;
  defaultPlies: number;
  configured: boolean;
  detail: AnalysisDetail;
  target: HTMLElement | null;
  onPosition: (id: string) => void;
  onTrackJob: (job: Job | undefined) => void;
  onCurrentJob: (job: Job | undefined) => void;
  onFilterChange: (filter: {
    player: string;
    kind: string;
    severity?: string;
    game_no?: number;
  }) => void;
  onShowErrors: () => void;
  selectedPositionId?: string;
}) {
  const onJob = useCallback(
    (job: Job | undefined) => {
      onCurrentJob(job);
      onTrackJob(job);
    },
    [onCurrentJob, onTrackJob],
  );
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
        gameNo={navigation.game?.game_no}
        defaultPlies={defaultPlies}
        configured={configured}
        controlsTarget={target}
        overview={detail === 'overview'}
        selectedPositionId={selectedPositionId}
        showResults={detail === 'overview' || detail === 'errors'}
        showStatistics={detail === 'overview' || detail === 'statistics'}
        onShowErrors={onShowErrors}
        onGame={(number) => void navigation.selectGame(number)}
        onJob={onJob}
        onAccepted={onTrackJob}
        onFilterChange={onFilterChange}
        onPosition={onPosition}
      />
    </div>
  );
}
