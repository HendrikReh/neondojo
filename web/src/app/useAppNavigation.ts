import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type MouseEvent,
} from 'react';
import { useWorkspaceDefaults } from '../settings/configuration/ConfigurationProvider';
import {
  sessionString,
  useWorkspaceSession,
} from '../shared/hooks/useWorkspaceSession';
import {
  readExtrasTool,
  saveExtrasTool,
} from '../workspaces/extras/extras-tool';
import {
  request,
  type Job,
  type Match,
  type Position,
} from '../shared/http/api';
import { AppError } from '../shared/i18n/i18n-core';
import type { AnalysisDetail } from '../workspaces/analysis/AnalysisWorkspace';
import type { PositionSearchRequest } from '../workspaces/library/MatchDatabase';
import type { AssessmentEntry } from '../workspaces/study/reports/match-assessment-api';
import type { AnnotationHit } from '../workspaces/analysis/notes/AnnotationSearch';
import type { AnnotationKind } from '../workspaces/analysis/notes/annotation-api';
import {
  PAUSED_MATCHES_DATABASE_ID,
  readDatabase,
  type StoredMove,
} from '../workspaces/library/library-api';
import type { PlaySessionRequest } from '../workspaces/play/usePlaySession';
import type { PlaySession } from '../workspaces/play/play-api';
import type { MoveNotesTarget } from '../workspaces/analysis/notes/MoveNotesDialog';
import { useGameNavigation } from './useGameNavigation';
import { usePositionKeys } from './usePositionKeys';
import { useSelectionRestore } from './useSelectionRestore';
import { clearSelection, readSelection } from './workspace-state';

export type AppView =
  | 'analysis'
  | 'database'
  | 'players'
  | 'positions'
  | 'learning'
  | 'play'
  | 'extras';

export function useAppNavigation({
  initialShowMatchList,
  settingsOpen,
  focusTab,
}: {
  initialShowMatchList: boolean;
  settingsOpen: boolean;
  focusTab: (view: AppView) => void;
}) {
  const legacy = useWorkspaceDefaults();
  const [positionScope, setPositionScope] = useState<'saved' | 'all'>('saved');
  // The decision search stays mounted after its first visit, so its filters
  // and results survive a switch to the saved positions and back.
  const [decisionSearchOpened, setDecisionSearchOpened] = useState(false);
  if (positionScope === 'all' && !decisionSearchOpened)
    setDecisionSearchOpened(true);

  const [extrasTool, setExtrasTool] = useState(readExtrasTool);
  useEffect(() => saveExtrasTool(extrasTool), [extrasTool]);

  const [restoredPlayId, setRestoredPlayId] = useState<string>();
  const [requestedPlaySession, setRequestedPlaySession] =
    useState<PlaySessionRequest>();
  const [requestedAssessment, setRequestedAssessment] =
    useState<AssessmentEntry>();
  const [requestedDeck, setRequestedDeck] = useState<{
    id: string;
    token: number;
  }>();
  const [requestedPositionSearch, setRequestedPositionSearch] =
    useState<PositionSearchRequest>();
  const [similarTarget, setSimilarTarget] = useState<{
    source: Position;
    matchName: string;
    token: number;
  }>();
  const [databaseId, setDatabaseId] = useWorkspaceSession(
    'library.database',
    legacy?.library.database_id,
    readDatabase(),
    sessionString,
  );
  const [moveNotes, setMoveNotes] = useState<MoveNotesTarget>();
  const annotationNavigation = useRef<AbortController | undefined>(undefined);
  useEffect(() => () => annotationNavigation.current?.abort(), []);

  const [view, setView] = useState<AppView | 'home'>(() => {
    if (window.location.hash === '#home') return 'home';
    if (initialShowMatchList) return 'database';
    return readSelection() ? 'analysis' : 'home';
  });
  useEffect(() => {
    const url = new URL(window.location.href);
    if (view === 'home') url.hash = 'home';
    else if (url.hash === '#home') url.hash = '';
    if (url.href !== window.location.href)
      window.history.replaceState(window.history.state, '', url);
  }, [view]);

  const [playSetupRequest, setPlaySetupRequest] = useState(0);
  const homeVisits = useRef(0);
  // Workspaces visited so far; they stay mounted while hidden.
  const [visited, setVisited] = useState<ReadonlySet<AppView | 'home'>>(
    () => new Set([view]),
  );
  useEffect(() => {
    setVisited((current) =>
      current.has(view) ? current : new Set(current).add(view),
    );
  }, [view]);
  const opened = (workspace: AppView) =>
    view === workspace || visited.has(workspace);

  const [detail, setDetail] = useState<AnalysisDetail>('overview');
  const [error, setError] = useState<unknown>();
  const [revision, setRevision] = useState(0);
  const [annotationRevision, setAnnotationRevision] = useState(0);
  const [deckRevision, setDeckRevision] = useState(0);
  const [bookmarkRevision, setBookmarkRevision] = useState(0);
  const [bookRevision, setBookRevision] = useState(0);
  const [requestedAnalysis, setRequestedAnalysis] = useState<Job>();
  // A cube decision chosen in the decision search; the analysis opens it once
  // its game has loaded.
  const [requestedCubeDecision, setRequestedCubeDecision] = useState<{
    matchId: string;
    gameNo: number;
    decisionNo: number;
    token: number;
  }>();
  const [scopeRequest, setScopeRequest] = useState<{
    kind: AnnotationKind;
    token: number;
  }>();

  const navigation = useGameNavigation();
  const [analysisOpened, setAnalysisOpened] = useState(false);
  if (navigation.match && !analysisOpened) setAnalysisOpened(true);

  const { restoring, cancelRestoration } = useSelectionRestore(
    navigation,
    (id) => {
      setRestoredPlayId(id);
      setRequestedPlaySession({ id, token: Date.now() });
      setView((current) => (current === 'analysis' ? 'play' : current));
    },
  );

  usePositionKeys(navigation, view === 'analysis' && !settingsOpen);
  useEffect(() => {
    if (view !== 'analysis' || settingsOpen)
      annotationNavigation.current?.abort();
  }, [view, settingsOpen]);

  const bumpRevision = useCallback(() => setRevision((n) => n + 1), []);
  const notesSaved = useCallback(() => {
    setAnnotationRevision((n) => n + 1);
    setRevision((n) => n + 1);
  }, []);
  const deckChanged = useCallback(() => setDeckRevision((n) => n + 1), []);
  const bookmarkChanged = useCallback(
    () => setBookmarkRevision((n) => n + 1),
    [],
  );
  const bumpBookRevision = useCallback(
    () => setBookRevision((n) => n + 1),
    [],
  );

  function selectTab(next: AppView) {
    cancelRestoration();
    setView(next);
  }

  function goHome(
    event: MouseEvent<HTMLAnchorElement>,
    closeImportNotice: () => void,
  ) {
    if (
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey
    )
      return;
    event.preventDefault();
    homeVisits.current++;
    annotationNavigation.current?.abort();
    setView('home');
    setError(undefined);
    closeImportNotice();
    setMoveNotes(undefined);
    window.scrollTo(0, 0);
  }

  function openMatch(match: Match, gameNo?: number, result?: Job) {
    cancelRestoration();
    setError(undefined);
    setView('analysis');
    setDetail(result ? 'statistics' : 'overview');
    setRequestedAnalysis(result);
    void (gameNo
      ? navigation.openMatch(match.id, gameNo)
      : navigation.selectMatch(match));
  }

  function searchCurrentPosition() {
    const positionId = navigation.position?.position_id;
    if (!positionId) return;
    cancelRestoration();
    setDatabaseId('');
    setRequestedPositionSearch((previous) => ({
      positionId,
      token: (previous?.token ?? 0) + 1,
    }));
    setView('database');
  }

  function searchSimilarPositions(source: Position, matchName: string) {
    setSimilarTarget((previous) => ({
      source,
      matchName,
      token: (previous?.token ?? 0) + 1,
    }));
  }

  function openPosition(position: Position | string) {
    cancelRestoration();
    setView('analysis');
    void navigation.openPosition(position);
  }

  function openAnnotation(hit: AnnotationHit) {
    cancelRestoration();
    annotationNavigation.current?.abort();
    const abort = new AbortController();
    annotationNavigation.current = abort;
    setView('analysis');
    setDetail('notes');
    if (hit.target_kind === 'move') {
      navigation.cancel();
      const intent = navigation.intent();
      void Promise.all([
        request<StoredMove>(`/moves/${hit.target_id}`, {
          signal: abort.signal,
        }),
        request<Match>(`/matches/${hit.match_uuid}`, { signal: abort.signal }),
      ])
        .then(([move, match]) => {
          if (abort.signal.aborted || navigation.intent() !== intent) return;
          setMoveNotes({ match, gameNo: move.game_no, moveId: move.id });
          void navigation.openMatch(match.id, move.game_no);
        })
        .catch((e) => {
          if (!abort.signal.aborted && navigation.intent() === intent)
            setError(e);
        });
    } else {
      setScopeRequest({ kind: hit.target_kind, token: Date.now() });
      void (hit.target_kind === 'position'
        ? navigation.openPosition(hit.target_id)
        : navigation.openMatch(hit.match_uuid, hit.game_no ?? undefined));
    }
  }

  function openStudyDeck(id: string) {
    cancelRestoration();
    deckChanged();
    setRequestedDeck((old) => ({ id, token: (old?.token ?? 0) + 1 }));
    setView('learning');
  }

  function playModeChanged(session: PlaySession) {
    if (session.play_mode === 'competition' && session.library_match_id) {
      navigation.invalidateMatchAnalysis(session.library_match_id);
    }
  }

  function openPlaySession(
    id: string,
    resume?: boolean,
    keepTurnAtVersion?: number,
  ) {
    cancelRestoration();
    setRequestedPlaySession((previous) => ({
      id,
      resume,
      keepTurnAtVersion,
      token: (previous?.token ?? 0) + 1,
    }));
    setView('play');
  }

  function openPlayAssessment(id: string) {
    cancelRestoration();
    setRequestedAssessment((previous) => ({
      id,
      token: (previous?.token ?? 0) + 1,
    }));
    setView('learning');
  }

  function openPausedMatches() {
    cancelRestoration();
    setDatabaseId(PAUSED_MATCHES_DATABASE_ID);
    setView('database');
    focusTab('database');
  }

  function openCubeDecision(decision: {
    matchId: string;
    gameNo: number;
    decisionNo: number;
  }) {
    cancelRestoration();
    setDetail('errors');
    setView('analysis');
    setRequestedCubeDecision((current) => ({
      ...decision,
      token: (current?.token ?? 0) + 1,
    }));
    void navigation.openMatch(decision.matchId, decision.gameNo);
  }

  function handleLibraryChanged(deletedIds: string[] = [], validate = false) {
    setRevision((n) => n + 1);
    if (validate && navigation.match) {
      const intent = navigation.intent(),
        id = navigation.match.id;
      void request(`/matches/${id}`).catch((e) => {
        if (
          e instanceof AppError &&
          String(e.params.status) === '404' &&
          navigation.intent() === intent
        ) {
          navigation.clear();
          clearSelection();
        }
      });
    }
    if (navigation.match && deletedIds.includes(navigation.match.id)) {
      navigation.clear();
      clearSelection();
    }
  }

  return {
    view,
    setView,
    selectTab,
    opened,
    detail,
    setDetail,
    positionScope,
    setPositionScope,
    decisionSearchOpened,
    analysisOpened,
    extrasTool,
    setExtrasTool,
    databaseId,
    setDatabaseId,
    restoredPlayId,
    setRestoredPlayId,
    requestedPlaySession,
    playSetupRequest,
    requestPlaySetup: () => setPlaySetupRequest((n) => n + 1),
    requestedAssessment,
    requestedDeck,
    requestedPositionSearch,
    similarTarget,
    setSimilarTarget,
    requestedAnalysis,
    setRequestedAnalysis,
    requestedCubeDecision,
    scopeRequest,
    moveNotes,
    setMoveNotes,
    error,
    setError,
    revision,
    bumpRevision,
    annotationRevision,
    deckRevision,
    bookmarkRevision,
    bookRevision,
    bumpBookRevision,
    notesSaved,
    deckChanged,
    bookmarkChanged,
    navigation,
    restoring,
    cancelRestoration,
    homeVisits,
    goHome,
    openMatch,
    searchCurrentPosition,
    searchSimilarPositions,
    openPosition,
    openAnnotation,
    openStudyDeck,
    playModeChanged,
    openPlaySession,
    openPlayAssessment,
    openPausedMatches,
    openCubeDecision,
    handleLibraryChanged,
  };
}
