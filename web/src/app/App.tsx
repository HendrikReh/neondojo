import {
  useConfirmedConfiguration,
  useWorkspaceDefaults,
} from '../settings/configuration/ConfigurationProvider';
import {
  sessionString,
  useWorkspaceSession,
} from '../shared/hooks/useWorkspaceSession';
import ProfileSelector from '../settings/profiles/ProfileSelector';
import { openingBookConfigurationKey } from '../settings/configuration/configuration-model';
import { settingsFromConfiguration } from '../settings/configuration/configuration-settings';
import { useBenchmarkActivity } from '../workspaces/extras/useBenchmarkActivity';
import {
  readExtrasTool,
  saveExtrasTool,
} from '../workspaces/extras/extras-tool';
import ActionButton from '../shared/ui/ActionButton';
import {
  lazy,
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';
import {
  request,
  type Job,
  type Match,
  type Position,
} from '../shared/http/api';
import type { AnalysisDetail } from '../workspaces/analysis/AnalysisWorkspace';
import MatchDatabase, {
  type PositionSearchRequest,
} from '../workspaces/library/MatchDatabase';
import type { AssessmentEntry } from '../workspaces/study/reports/match-assessment-api';
import { readTheme, saveTheme } from '../settings/ui-preferences';
import type { Theme } from '../shared/board/themes';
import { useGameNavigation } from './useGameNavigation';
import { useJobs } from '../shared/jobs/useJobs';
import { useBackgroundJobs } from './useBackgroundJobs';
import { useI18n } from '../shared/i18n/i18n';
import { useDetailsDismiss } from '../shared/hooks/useDetailsDismiss';
import { useWorkspaceText } from '../shared/i18n/workspace-messages';
import {
  decodeBulkAnalysisSession,
  clearSelection,
  readSelection,
} from './workspace-state';
import { AppError } from '../shared/i18n/i18n-core';
import type { AnnotationHit } from '../workspaces/analysis/notes/AnnotationSearch';
import type { AnnotationKind } from '../workspaces/analysis/notes/annotation-api';
import './workspace.css';
import {
  PAUSED_MATCHES_DATABASE_ID,
  readDatabase,
  type StoredMove,
  type LibraryMatch,
} from '../workspaces/library/library-api';
import type { PlaySessionRequest } from '../workspaces/play/usePlaySession';
import type { PlaySession } from '../workspaces/play/play-api';
import AppFooter from './AppFooter';
import { DEFAULT_ANALYSIS_PLIES } from '../shared/engine/analysis-depth';
import type { MoveNotesTarget } from '../workspaces/analysis/notes/MoveNotesDialog';
import { useUiAppearance } from '../settings/appearance/useUiAppearance';
import { useButtonShortcuts } from './useButtonShortcuts';
import LazyPanel, { MountHidden } from './LazyPanel';
import type { AnalysisSelection } from './AnalysisDialogs';
import AppNotices from './AppNotices';
import AppWelcome from './AppWelcome';
import MatchImportInputs from './MatchImportInputs';
import { bulkJobPending, useBulkAnalysis } from './useBulkAnalysis';
import { useInstalledSettings } from './useInstalledSettings';
import { useMatchImporter } from './useMatchImporter';
import { usePositionKeys } from './usePositionKeys';
import { useSelectionRestore } from './useSelectionRestore';

const HomePage = lazy(() => import('./HomePage'));
const AnalysisWorkspace = lazy(
  () => import('../workspaces/analysis/AnalysisWorkspace'),
);
const AnalysisDialogs = lazy(() => import('./AnalysisDialogs'));
const MoveNotesDialog = lazy(
  () => import('../workspaces/analysis/notes/MoveNotesDialog'),
);
const PlayWorkspace = lazy(() => import('../workspaces/play/PlayWorkspace'));
const PlayerDirectory = lazy(
  () => import('../workspaces/players/PlayerDirectory'),
);
const StudyWorkspace = lazy(() => import('../workspaces/study/StudyWorkspace'));
const PositionBrowser = lazy(
  () => import('../workspaces/positions/PositionBrowser'),
);
const SimilarPositions = lazy(
  () => import('../workspaces/positions/SimilarPositions'),
);
const DecisionSearch = lazy(
  () => import('../workspaces/positions/DecisionSearch'),
);
const ExtrasWorkspace = lazy(
  () => import('../workspaces/extras/ExtrasWorkspace'),
);
const SettingsDialog = lazy(() => import('../settings/SettingsDialog'));

type View =
  | 'analysis'
  | 'database'
  | 'players'
  | 'positions'
  | 'learning'
  | 'play'
  | 'extras';
const views: View[] = [
  'play',
  'analysis',
  'learning',
  'positions',
  'database',
  'players',
  'extras',
];
export default function App() {
  useButtonShortcuts();
  const helpMenu = useDetailsDismiss();
  const [positionScope, setPositionScope] = useState<'saved' | 'all'>('saved');
  // The decision search stays mounted after its first visit, so its filters
  // and results survive a switch to the saved positions and back.
  const [decisionSearchOpened, setDecisionSearchOpened] = useState(false);
  if (positionScope === 'all' && !decisionSearchOpened)
    setDecisionSearchOpened(true);
  const confirmed = useConfirmedConfiguration();
  const legacy = useWorkspaceDefaults();
  const [settingsCategory, setSettingsCategory] = useState<
    'general' | 'play' | 'profiles'
  >('general');
  const benchmarkActivity = useBenchmarkActivity();
  const [extrasTool, setExtrasTool] = useState(readExtrasTool);
  useEffect(() => saveExtrasTool(extrasTool), [extrasTool]);
  const [restoredPlayId, setRestoredPlayId] = useState<string>();
  const [requestedPlaySession, setRequestedPlaySession] =
    useState<PlaySessionRequest>();
  const { t, setLocale } = useI18n();
  const w = useWorkspaceText();
  const { appearance, applyAppearance } = useUiAppearance();
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
  const [bulkAnalysis, setBulkAnalysis] = useWorkspaceSession(
    'library.bulkAnalysis',
    undefined,
    { jobs: {}, showMatchList: false },
    decodeBulkAnalysisSession,
  );
  const bulkJobIds = Object.keys(bulkAnalysis.jobs);
  const [view, setView] = useState<View | 'home'>(() => {
    if (window.location.hash === '#home') return 'home';
    if (bulkAnalysis.showMatchList && bulkJobIds.length) return 'database';
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
  const [visited, setVisited] = useState<ReadonlySet<View | 'home'>>(
    () => new Set([view]),
  );
  useEffect(() => {
    setVisited((current) =>
      current.has(view) ? current : new Set(current).add(view),
    );
  }, [view]);
  const opened = (workspace: View) =>
    view === workspace || visited.has(workspace);
  const [detail, setDetail] = useState<AnalysisDetail>('overview');
  const [localTheme, setTheme] = useState<Theme>(readTheme);
  const theme = confirmed?.appearance.board_theme ?? localTheme;
  const {
    settings: installedSettings,
    error: settingsError,
    revision: settingsRevision,
    reload: reloadSettings,
    apply: applyInstalledSettings,
  } = useInstalledSettings();
  const settings =
    confirmed && installedSettings
      ? settingsFromConfiguration(confirmed, installedSettings)
      : installedSettings;
  const analysisEngine =
    confirmed?.analysis.engine ?? settings?.analysis_engine ?? 'gnubg';
  const analysisPlies =
    confirmed?.analysis.plies ??
    settings?.analysis_plies ??
    DEFAULT_ANALYSIS_PLIES;
  const [settingsOpen, setSettingsOpen] = useState(false);
  // The dialog mounts when first opened and then stays mounted.
  const [settingsOpened, setSettingsOpened] = useState(false);
  useEffect(() => {
    if (settingsOpen) setSettingsOpened(true);
  }, [settingsOpen]);
  const [bookRevision, setBookRevision] = useState(0);
  const bookConfigurationKey = openingBookConfigurationKey(
    confirmed?.opening_book,
  );
  const footerEngine =
    view === 'play'
      ? (confirmed?.play.engine ?? settings?.play_engine ?? 'gnubg')
      : analysisEngine;
  const [error, setError] = useState<unknown>();
  const [revision, setRevision] = useState(0);
  const [annotationRevision, setAnnotationRevision] = useState(0);
  const [deckRevision, setDeckRevision] = useState(0);
  const [bookmarkRevision, setBookmarkRevision] = useState(0);
  const [analysisSelection, setAnalysisSelection] =
    useState<AnalysisSelection>();
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
  const importer = useMatchImporter({
    databaseId,
    intent: () => navigation.intent() + homeVisits.current,
    onStart: cancelRestoration,
    onOpen: (match) => {
      setView('analysis');
      setDetail('overview');
      void navigation.selectMatch(match);
    },
    onChanged: () => setRevision((n) => n + 1),
    onBatchFinished: () => setView('database'),
    onError: setError,
  });
  const {
    active: runningAnalysisJobs,
    tracked: trackedAnalysisJobs,
    track,
    missing: missingAnalysisJobs,
    assessmentRevision,
  } = useBackgroundJobs(bulkJobIds);
  const bulkAnalysisProgress = useBulkAnalysis({
    session: bulkAnalysis,
    setSession: setBulkAnalysis,
    databaseId,
    tracked: trackedAnalysisJobs,
    missing: missingAnalysisJobs,
    listVisible: view === 'database',
  });
  const analysisInstallationKey = JSON.stringify([
    installedSettings?.version,
    installedSettings?.gnubg_path,
    installedSettings?.analysis_engine,
    settingsRevision,
  ]);
  const jobs = useJobs(navigation.position?.id, analysisEngine, track);
  const tabs = useRef<Partial<Record<View, HTMLButtonElement | null>>>({});
  useEffect(() => {
    if (!confirmed) saveTheme(theme);
  }, [theme, !!confirmed]);
  useEffect(() => {
    const current = confirmed;
    if (!current) return;
    setLocale(current.general.locale);
  }, [confirmed?.general.locale, setLocale]);
  useEffect(() => {
    if (bookConfigurationKey !== undefined) setBookRevision((n) => n + 1);
  }, [bookConfigurationKey]);
  useEffect(() => {
    Object.values(jobs.jobs).forEach(track);
  }, [jobs.jobs, track]);
  function playModeChanged(session: PlaySession) {
    if (session.play_mode === 'competition' && session.library_match_id) {
      navigation.invalidateMatchAnalysis(session.library_match_id);
    }
  }
  usePositionKeys(navigation, view === 'analysis' && !settingsOpen);
  function tabKey(event: KeyboardEvent<HTMLButtonElement>, current: View) {
    let next: number;
    const index = views.indexOf(current);
    if (event.key === 'ArrowRight') next = (index + 1) % views.length;
    else if (event.key === 'ArrowLeft')
      next = (index + views.length - 1) % views.length;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = views.length - 1;
    else return;
    event.preventDefault();
    cancelRestoration();
    setView(views[next]);
    tabs.current[views[next]]?.focus();
  }
  function goHome(event: MouseEvent<HTMLAnchorElement>) {
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
    importer.closeNotice();
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
  useEffect(() => {
    if (view !== 'analysis' || settingsOpen)
      annotationNavigation.current?.abort();
  }, [view, settingsOpen]);
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
  const notesSaved = useCallback(() => {
    setAnnotationRevision((n) => n + 1);
    setRevision((n) => n + 1);
  }, []);
  const deckChanged = useCallback(() => setDeckRevision((n) => n + 1), []);
  function openStudyDeck(id: string) {
    cancelRestoration();
    deckChanged();
    setRequestedDeck((old) => ({ id, token: (old?.token ?? 0) + 1 }));
    setView('learning');
  }
  const bookmarkChanged = useCallback(
    () => setBookmarkRevision((n) => n + 1),
    [],
  );
  const welcome = (
    <AppWelcome
      importer={importer}
      restoring={restoring}
      onOpenLibrary={() => {
        cancelRestoration();
        setView('database');
      }}
    />
  );
  const selectForAnalysis = (
    matches: LibraryMatch[],
    unavailableCount: number,
  ) =>
    setAnalysisSelection(
      matches.length === 1 && !unavailableCount
        ? { kind: 'single', match: matches[0] }
        : { kind: 'bulk', matches, unavailableCount },
    );
  // Shown by the workspace that asked for the analysis.
  const analysisDialogs = analysisSelection && (
    <LazyPanel>
      <AnalysisDialogs
        selection={analysisSelection}
        engine={analysisEngine}
        plies={analysisPlies}
        installationKey={analysisInstallationKey}
        trackedJobs={trackedAnalysisJobs}
        onAccepted={(jobs) => {
          for (const job of jobs) track(job);
          setRevision((value) => value + 1);
        }}
        onBulkAccepted={(databases, continuesBatch) =>
          setBulkAnalysis((previous) => {
            const overlaps = Object.keys(previous.jobs).some((id) =>
              bulkJobPending(id, trackedAnalysisJobs, missingAnalysisJobs),
            );
            return {
              jobs: {
                ...(continuesBatch || overlaps ? previous.jobs : {}),
                ...databases,
              },
              showMatchList: true,
            };
          })
        }
        onShowResults={(match, job) => {
          setAnalysisSelection(undefined);
          openMatch(match, undefined, job);
          requestAnimationFrame(() => tabs.current.analysis?.focus());
        }}
        onClose={() => setAnalysisSelection(undefined)}
      />
    </LazyPanel>
  );

  return (
    <div className="app-shell">
      <header className="app-header">
        <a
          className="brand"
          href="/#home"
          aria-current={view === 'home' ? 'page' : undefined}
          aria-label={t('app.home')}
          onClick={goHome}
        >
          <img
            className="brand-mark"
            src="/neondojo-logo.svg"
            alt=""
            width={30}
            height={30}
          />
          <span>NeonDojo</span>
        </a>
        <div className="app-header-navigation">
          <nav
            className="app-tabs text-navigation"
            role="tablist"
            aria-label={w('navigation')}
          >
            {views.map((tab) => (
              <button
                key={tab}
                type="button"
                role="tab"
                id={`workspace-tab-${tab}`}
                aria-controls={`workspace-panel-${tab}`}
                aria-selected={view === tab}
                tabIndex={
                  view === tab || (view === 'home' && tab === views[0]) ? 0 : -1
                }
                ref={(el) => {
                  tabs.current[tab] = el;
                }}
                onKeyDown={(event) => tabKey(event, tab)}
                onClick={() => {
                  cancelRestoration();
                  setView(tab);
                }}
              >
                {w(tab)}
              </button>
            ))}
          </nav>
          <details
            className="app-help text-navigation"
            ref={helpMenu.ref}
            onKeyDown={helpMenu.onKeyDown}
          >
            <summary>{t('app.help')}</summary>
            <div className="app-help-links">
              <a
                href="https://hendrikreh.github.io/neondojo/user-guide/"
                target="_blank"
                rel="noreferrer"
              >
                {t('app.neondojoManual')}
              </a>
              <a
                href="https://hendrikreh.github.io/neongammon/"
                target="_blank"
                rel="noreferrer"
              >
                {t('app.neongammonManual')}
              </a>
            </div>
          </details>
        </div>
        <div className="app-header-actions">
          <ProfileSelector
            onManage={() => {
              setSettingsCategory('profiles');
              setSettingsOpen(true);
            }}
          />
          <ActionButton
            className="settings-trigger"
            onClick={() => {
              setSettingsCategory('general');
              setSettingsOpen(true);
            }}
            icon="settings"
            label={w('settings')}
            compact
          ></ActionButton>
        </div>
        <MatchImportInputs importer={importer} />
      </header>
      {benchmarkActivity.active && (
        <p className="benchmark-global-status" role="status">
          {w('globalBusy')}
        </p>
      )}
      <main className={`app-body app-body-${view}`}>
        <AppNotices
          error={error || navigation.error}
          settingsError={settingsError}
          importer={importer}
          onCloseError={() => {
            setError(undefined);
            navigation.clearError();
          }}
          onRetrySettings={reloadSettings}
        />
        {view === 'home' && (
          <LazyPanel>
            <HomePage
              match={navigation.match}
              position={navigation.gameLoaded ? navigation.position : undefined}
              theme={theme}
              loading={restoring || navigation.loading}
              importer={importer}
              restoredPlayId={restoredPlayId}
              onContinue={() => {
                setDetail('overview');
                setView('analysis');
              }}
              onOpenLibrary={() => {
                cancelRestoration();
                setView('database');
              }}
              onPlay={() => {
                cancelRestoration();
                setPlaySetupRequest((n) => n + 1);
                setView('play');
              }}
              onResumePlay={(session) => {
                cancelRestoration();
                setRequestedPlaySession((previous) => ({
                  id: session.id,
                  resume: session.paused,
                  keepTurnAtVersion: session.version,
                  token: (previous?.token ?? 0) + 1,
                }));
                setView('play');
              }}
              onSearch={() => {
                cancelRestoration();
                setSimilarTarget(undefined);
                setPositionScope('all');
                setView('positions');
              }}
              onStudy={openStudyDeck}
            />
          </LazyPanel>
        )}
        {/* Global errors, notices and import progress stay above every workspace. */}
        <section
          className="workspace-panel"
          id="workspace-panel-play"
          role="tabpanel"
          aria-labelledby="workspace-tab-play"
          hidden={view !== 'play'}
        >
          <LazyPanel>
            {opened('play') && (
              <MountHidden visible={view === 'play'}>
                {(visible) => (
                  <PlayWorkspace
                    visible={visible}
                    settings={settings}
                    theme={theme}
                    requestedSession={requestedPlaySession}
                    setupRequest={playSetupRequest}
                    onSaved={() => {
                      setRestoredPlayId(undefined);
                      setRevision((n) => n + 1);
                    }}
                    onModeChanged={playModeChanged}
                    onOpenPlaySettings={() => {
                      setSettingsCategory('play');
                      setSettingsOpen(true);
                    }}
                    onOpenAssessment={(id) => {
                      cancelRestoration();
                      setRequestedAssessment((previous) => ({
                        id,
                        token: (previous?.token ?? 0) + 1,
                      }));
                      setView('learning');
                    }}
                    onOpenPausedMatches={() => {
                      cancelRestoration();
                      setDatabaseId(PAUSED_MATCHES_DATABASE_ID);
                      setView('database');
                      tabs.current.database?.focus();
                    }}
                  />
                )}
              </MountHidden>
            )}
          </LazyPanel>
        </section>
        <div
          id="workspace-panel-analysis"
          role="tabpanel"
          aria-labelledby="workspace-tab-analysis"
          hidden={view !== 'analysis'}
        >
          {analysisOpened ? (
            <LazyPanel>
              <AnalysisWorkspace
                active={view === 'analysis'}
                requestedAnalysis={requestedAnalysis}
                requestedCubeDecision={requestedCubeDecision}
                onAnalysisRequestConsumed={(consumed) =>
                  setRequestedAnalysis((pending) =>
                    pending === consumed ? undefined : pending,
                  )
                }
                analysisRevision={JSON.stringify([
                  assessmentRevision,
                  runningAnalysisJobs
                    .filter(
                      (job) => job.kind === 'match' && job.game_no == null,
                    )
                    .map((job) => job.id)
                    .sort(),
                  confirmed?.analysis.error_thresholds,
                ])}
                metadataRevision={revision}
                bookmarkRevision={bookmarkRevision}
                onBookmarkChanged={bookmarkChanged}
                navigation={navigation}
                jobs={jobs}
                runningJobs={runningAnalysisJobs}
                theme={theme}
                defaultEngine={analysisEngine}
                defaultPlies={analysisPlies}
                analysisInstallationKey={analysisInstallationKey}
                openingBookRevision={bookRevision}
                configured
                detail={detail}
                onDetail={setDetail}
                onOpenDatabase={searchCurrentPosition}
                onAnalyzeMatch={(match) =>
                  // The Analysis header names the players, like the dialog's fallback.
                  setAnalysisSelection({
                    kind: 'single',
                    match: { ...match, display_name: '' },
                  })
                }
                onTrackJob={track}
                onNotesSaved={notesSaved}
                notesSuspended={!!moveNotes || view !== 'analysis'}
                onMoveNotes={(target) => {
                  if (navigation.match && navigation.game)
                    setMoveNotes({
                      match: navigation.match,
                      gameNo: target?.gameNo ?? navigation.game.game_no,
                      moveId: target?.moveId,
                      decisionKind: target?.decisionKind,
                      reviewDecision: target?.reviewDecision,
                    });
                }}
                deckRevision={deckRevision}
                onDeckChanged={deckChanged}
                onPosition={openPosition}
                scopeRequest={scopeRequest}
                welcome={welcome}
              />
            </LazyPanel>
          ) : (
            view === 'analysis' && (
              <section className="analysis-workspace">{welcome}</section>
            )
          )}
          {view === 'analysis' && analysisDialogs}
        </div>
        <div
          id="workspace-panel-database"
          role="tabpanel"
          aria-labelledby="workspace-tab-database"
          hidden={view !== 'database'}
        >
          <MatchDatabase
            onOpenPlay={(id, resume) => {
              cancelRestoration();
              setRequestedPlaySession((previous) => ({
                id,
                resume,
                token: (previous?.token ?? 0) + 1,
              }));
              setView('play');
            }}
            revision={revision}
            databaseId={databaseId}
            analysisRevision={assessmentRevision}
            onDatabase={setDatabaseId}
            visible={view === 'database'}
            onChanged={(deletedIds = [], validateSelection = false) => {
              setRevision((n) => n + 1);
              if (validateSelection && navigation.match) {
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
              if (
                navigation.match &&
                deletedIds.includes(navigation.match.id)
              ) {
                navigation.clear();
                clearSelection();
              }
            }}
            annotationRevision={annotationRevision}
            currentPosition={navigation.position}
            requestedPositionSearch={requestedPositionSearch}
            selectedMatchId={navigation.match?.id}
            onOpen={openMatch}
            bulkAnalysisProgress={bulkAnalysisProgress}
            onAnalyzeMatches={selectForAnalysis}
            onPosition={openPosition}
            onAnnotation={openAnnotation}
            uploading={importer.uploading}
            onImport={importer.chooseFiles}
            onImportFolder={importer.chooseFolder}
          />
          {view === 'database' && analysisDialogs}
        </div>
        <div
          id="workspace-panel-players"
          role="tabpanel"
          aria-labelledby="workspace-tab-players"
          hidden={view !== 'players'}
        >
          <LazyPanel>
            {opened('players') && (
              <PlayerDirectory
                revision={revision}
                onOpen={openMatch}
                onAnalyzeMatches={selectForAnalysis}
              />
            )}
          </LazyPanel>
          {view === 'players' && analysisDialogs}
        </div>
        <div
          id="workspace-panel-positions"
          role="tabpanel"
          aria-labelledby="workspace-tab-positions"
          hidden={view !== 'positions'}
        >
          <div
            className="text-navigation position-scope"
            role="group"
            aria-label={w('scope')}
          >
            {/* The scopes stay available beside a similar-position search;
                choosing one closes that search. */}
            {(['saved', 'all'] as const).map((scope) => (
              <button
                key={scope}
                type="button"
                aria-pressed={!similarTarget && positionScope === scope}
                onClick={() => {
                  setSimilarTarget(undefined);
                  setPositionScope(scope);
                }}
              >
                {w(scope)}
              </button>
            ))}
          </div>
          <LazyPanel>
            {decisionSearchOpened && (
              <div hidden={!!similarTarget || positionScope !== 'all'}>
                <DecisionSearch
                  visible={
                    view === 'positions' &&
                    !similarTarget &&
                    positionScope === 'all'
                  }
                  revision={revision}
                  bookmarkRevision={bookmarkRevision}
                  onBookmarkChanged={bookmarkChanged}
                  deckRevision={deckRevision}
                  onDeckChanged={deckChanged}
                  theme={theme}
                  onOpenPosition={(id) => {
                    setDetail('overview');
                    openPosition(id);
                  }}
                  onOpenCubeDecision={(decision) => {
                    cancelRestoration();
                    setDetail('errors');
                    setView('analysis');
                    setRequestedCubeDecision((current) => ({
                      ...decision,
                      token: (current?.token ?? 0) + 1,
                    }));
                    void navigation.openMatch(
                      decision.matchId,
                      decision.gameNo,
                    );
                  }}
                />
              </div>
            )}
          </LazyPanel>
          <div hidden={!!similarTarget || positionScope === 'all'}>
            <LazyPanel>
              {opened('positions') && (
                <MountHidden
                  visible={
                    view === 'positions' &&
                    !similarTarget &&
                    positionScope === 'saved'
                  }
                >
                  {(visible) => (
                    <PositionBrowser
                      visible={visible}
                      bookmarkRevision={bookmarkRevision}
                      onBookmarkChanged={bookmarkChanged}
                      revision={revision}
                      annotationRevision={annotationRevision}
                      deckRevision={deckRevision}
                      theme={theme}
                      onOpenPosition={(id) => {
                        setDetail('overview');
                        openPosition(id);
                      }}
                      onDeckChanged={deckChanged}
                      onStudy={openStudyDeck}
                      onFindSimilar={searchSimilarPositions}
                    />
                  )}
                </MountHidden>
              )}
            </LazyPanel>
          </div>
          <LazyPanel>
            {similarTarget && (
              <SimilarPositions
                key={similarTarget.token}
                source={similarTarget.source}
                matchName={similarTarget.matchName}
                visible={view === 'positions'}
                revision={revision}
                theme={theme}
                onClose={() => setSimilarTarget(undefined)}
                onOpen={(id) => {
                  setDetail('overview');
                  openPosition(id);
                }}
              />
            )}
          </LazyPanel>
        </div>
        <div
          id="workspace-panel-learning"
          role="tabpanel"
          aria-labelledby="workspace-tab-learning"
          hidden={view !== 'learning'}
        >
          <LazyPanel>
            {opened('learning') && (
              <MountHidden visible={view === 'learning'}>
                {(visible) => (
                  <StudyWorkspace
                    visible={visible}
                    requestedDeck={requestedDeck}
                    requestedAssessment={requestedAssessment}
                    theme={theme}
                    revision={deckRevision}
                    onChanged={deckChanged}
                    onStudyDeck={openStudyDeck}
                    onOpenPlayers={() => setView('players')}
                  />
                )}
              </MountHidden>
            )}
          </LazyPanel>
        </div>
        <div
          id="workspace-panel-extras"
          role="tabpanel"
          aria-labelledby="workspace-tab-extras"
          hidden={view !== 'extras'}
        >
          <LazyPanel>
            {opened('extras') && (
              <MountHidden visible={view === 'extras'}>
                {(visible) => (
                  <ExtrasWorkspace
                    visible={visible}
                    tool={extrasTool}
                    setTool={setExtrasTool}
                    onOpenMatch={openMatch}
                    defaultPlies={analysisPlies}
                    playPlies={settings?.play_plies}
                    benchmarkActive={benchmarkActivity.active}
                    onBenchmarkActivity={benchmarkActivity.report}
                  />
                )}
              </MountHidden>
            )}
          </LazyPanel>
        </div>
        <AppFooter
          key={footerEngine}
          engine={footerEngine}
          settingsOpen={settingsOpen}
          settingsVersion={settings?.version}
        />
      </main>
      {moveNotes && (
        <LazyPanel>
          <MoveNotesDialog
            target={moveNotes}
            theme={theme}
            onClose={() => setMoveNotes(undefined)}
            onSaved={notesSaved}
          />
        </LazyPanel>
      )}
      <LazyPanel quiet>
        {(settingsOpen || settingsOpened) && (
          <SettingsDialog
            initialCategory={settingsCategory}
            open={settingsOpen}
            onClose={() => setSettingsOpen(false)}
            onBookRefresh={() => setBookRevision((n) => n + 1)}
            onBenchmark={() => {
              cancelRestoration();
              setSettingsOpen(false);
              setExtrasTool('benchmark');
              setView('extras');
            }}
            theme={theme}
            appearance={appearance}
            onApply={(applied) => {
              setTheme(applied.theme);
              setLocale(applied.locale);
              applyAppearance(applied.appearance);
              applyInstalledSettings(applied.settings);
              if (applied.myProfileChanged) setRevision((n) => n + 1);
            }}
          />
        )}
      </LazyPanel>
    </div>
  );
}
