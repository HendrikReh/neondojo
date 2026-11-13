import { useOptionalConfiguration } from './ConfigurationProvider';
import ProfileSelector from './ProfileSelector';
import { settingsFromConfiguration } from './configuration-settings';
import { loadBenchmarkStatus } from './benchmark-api';
import ExtrasWorkspace, {
  readExtrasTool,
  saveExtrasTool,
} from './ExtrasWorkspace';
import { useBenchmarkText } from './benchmark-messages';
import ActionButton from './ActionButton';
import PlayWorkspace from './PlayWorkspace';
import MatchDropZone from './MatchDropZone';
import { usePlayText } from './play-messages';
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';
import { request, type Match, type Position } from './api';
import AnalysisWorkspace, { type AnalysisDetail } from './AnalysisWorkspace';
import PlayerDirectory from './PlayerDirectory';
import { usePlayerText } from './player-directory-messages';
import MatchDatabase, { type PositionSearchRequest } from './MatchDatabase';
import StudyWorkspace from './StudyWorkspace';
import PositionBrowser from './PositionBrowser';
import SimilarPositions from './SimilarPositions';
import { usePositionText } from './position-browser-messages';
import SettingsDialog from './SettingsDialog';
import { loadSettings, type AppSettings } from './settings-api';
import { readTheme, saveTheme } from './ui-preferences';
import type { Theme } from './themes';
import { useGameNavigation } from './useGameNavigation';
import { useJobs } from './useJobs';
import { useBackgroundJobs } from './useBackgroundJobs';
import { useI18n } from './i18n';
import { useWorkspaceText } from './workspace-messages';
import { useLearningText } from './learning-messages';
import { readSelection, saveSelection } from './workspace-state';
import { AppError } from './i18n-core';
import type { AnnotationHit } from './AnnotationSearch';
import type { AnnotationKind } from './annotation-api';
import './workspace.css';
import {
  acceptsLibraryMatches,
  PAUSED_MATCHES_DATABASE_ID,
  readDatabase,
  saveDatabase,
  type StoredMove,
  type LibraryMatch,
} from './library-api';
import { useLibraryText } from './library-messages';
import { isUnfinishedCompetition } from './library-match-summary';
import type { PlaySession } from './play-api';
import { DEFAULT_ANALYSIS_PLIES } from './analysis-depth';
import MoveNotesDialog, { type MoveNotesTarget } from './MoveNotesDialog';
import { useUiAppearance } from './useUiAppearance';

type View =
  | 'analysis'
  | 'database'
  | 'players'
  | 'positions'
  | 'learning'
  | 'play'
  | 'extras';
const views: View[] = [
  'analysis',
  'positions',
  'learning',
  'play',
  'database',
  'players',
  'extras',
];
export default function App() {
  const configuration = useOptionalConfiguration();
  const [settingsCategory, setSettingsCategory] = useState<
    'general' | 'profiles'
  >('general');
  const playText = usePlayText();
  const benchmarkT = useBenchmarkText();
  const [benchmarkActive, setBenchmarkActive] = useState(false);
  const [extrasTool, setExtrasTool] = useState(readExtrasTool);
  useEffect(() => saveExtrasTool(extrasTool), [extrasTool]);
  useEffect(() => {
    const abort = new AbortController();
    let timer: ReturnType<typeof setTimeout>;
    async function pollBenchmarkStatus() {
      try {
        const status = await loadBenchmarkStatus(abort.signal);
        if (!abort.signal.aborted) setBenchmarkActive(status.active === true);
      } catch {
        // Status is advisory; a temporary failure must not block the app.
      } finally {
        if (!abort.signal.aborted)
          timer = setTimeout(() => void pollBenchmarkStatus(), 2000);
      }
    }
    void pollBenchmarkStatus();
    return () => {
      abort.abort();
      clearTimeout(timer);
    };
  }, []);
  const [requestedPlaySession, setRequestedPlaySession] = useState<{
    id: string;
    token: number;
    resume?: boolean;
  }>();
  const { t, setLocale, formatError } = useI18n();
  const w = useWorkspaceText(),
    learnT = useLearningText();
  const l = useLibraryText();
  const playerText = usePlayerText();
  const [playersOpened, setPlayersOpened] = useState(false);
  const positionText = usePositionText();
  const { appearance, applyAppearance } = useUiAppearance();
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
  const [databaseId, setDatabaseId] = useState(readDatabase);
  useEffect(() => {
    if (configuration?.current)
      setDatabaseId(configuration.current.library.database_id);
  }, [configuration?.current?.library.database_id]);
  const changeDatabase = (id: string) => {
    setDatabaseId(id);
    configuration?.updateCurrent((c) => ({
      ...c,
      library: { ...c.library, database_id: id },
    }));
  };
  const [moveNotes, setMoveNotes] = useState<MoveNotesTarget>();
  const [importResults, setImportResults] = useState<
    {
      name: string;
      status: 'importedFile' | 'duplicate' | 'error';
      detail?: unknown;
    }[]
  >([]);
  const annotationNavigation = useRef<AbortController | undefined>(undefined);
  useEffect(() => {
    if (!configuration) saveDatabase(databaseId);
  }, [databaseId, !!configuration]);
  useEffect(() => () => annotationNavigation.current?.abort(), []);
  const [view, setView] = useState<View>('analysis');
  useEffect(() => {
    if (view === 'players') setPlayersOpened(true);
  }, [view]);
  const [detail, setDetail] = useState<AnalysisDetail>('overview');
  const [localTheme, setTheme] = useState<Theme>(readTheme);
  const theme = configuration?.current?.appearance.board_theme ?? localTheme;
  const [installedSettings, setInstalledSettings] = useState<AppSettings>();
  const settings =
    configuration?.current && installedSettings
      ? settingsFromConfiguration(configuration.current, installedSettings)
      : installedSettings;
  const [settingsError, setSettingsError] = useState<unknown>();
  const [settingsRevision, setSettingsRevision] = useState(0);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [bookRevision, setBookRevision] = useState(0);
  const [health, setHealth] = useState<'checking' | 'ok' | 'degraded'>(
    'checking',
  );
  const [error, setError] = useState<unknown>();
  const [notice, setNotice] = useState<{ duplicate: boolean; count: number }>();
  const [uploading, setUploading] = useState(false);
  const [revision, setRevision] = useState(0);
  const [annotationRevision, setAnnotationRevision] = useState(0);
  const [deckRevision, setDeckRevision] = useState(0);
  const [bookmarkRevision, setBookmarkRevision] = useState(0);
  const [scopeRequest, setScopeRequest] = useState<{
    kind: AnnotationKind;
    token: number;
  }>();
  const [restoring, setRestoring] = useState(false);
  const restorationRequest = useRef<AbortController | null>(null);
  const navigation = useGameNavigation();
  const { track, assessmentRevision } = useBackgroundJobs();
  const jobs = useJobs(navigation.position?.id, track);
  const settingsLoad = useRef<AbortController | undefined>(undefined);
  const input = useRef<HTMLInputElement>(null);
  const tabs = useRef<Partial<Record<View, HTMLButtonElement | null>>>({});
  useEffect(() => {
    if (!configuration) saveTheme(theme);
  }, [theme, !!configuration]);
  useEffect(() => {
    const current = configuration?.current;
    if (!current) return;
    setLocale(current.general.locale);
  }, [configuration?.current?.general.locale, setLocale]);
  useEffect(() => {
    if (configuration?.current) setBookRevision((n) => n + 1);
  }, [
    configuration?.current?.opening_book.provider,
    configuration?.current?.opening_book.path,
    configuration?.current?.opening_book.xg_path,
  ]);
  useEffect(() => {
    Object.values(jobs.jobs).forEach(track);
  }, [jobs.jobs, track]);
  useEffect(() => {
    const abort = new AbortController();
    settingsLoad.current = abort;
    void loadSettings(abort.signal)
      .then((value) => {
        if (abort.signal.aborted) return;
        setInstalledSettings((current) =>
          current && current.version > value.version ? current : value,
        );
        setSettingsError(undefined);
      })
      .catch((e) => {
        if (!abort.signal.aborted) setSettingsError(e);
      });
    return () => abort.abort();
  }, [settingsRevision]);
  useEffect(() => {
    const abort = new AbortController();
    void request<{ status: string }>('/health', { signal: abort.signal })
      .then((value) => setHealth(value.status === 'ok' ? 'ok' : 'degraded'))
      .catch(() => {
        if (!abort.signal.aborted) setHealth('degraded');
      });
    return () => abort.abort();
  }, [settings?.version]);
  useEffect(() => {
    const previous = readSelection();
    if (!previous) return;
    const abort = new AbortController();
    restorationRequest.current = abort;
    const intent = navigation.intent();
    const current = () =>
      !abort.signal.aborted && navigation.intent() === intent;
    setRestoring(true);
    void (async () => {
      // Metadata is available even while a Competition source is restricted.
      // Do not restore that source through the analysis endpoints.
      let metadata: LibraryMatch | undefined;
      try {
        metadata = await request<LibraryMatch>(
          `/matches/${previous.matchId}/metadata`,
          { signal: abort.signal },
        );
      } catch {
        // Old/missing metadata still uses normal navigation and its real error.
      }
      if (!current()) return;
      if (
        metadata &&
        (metadata.play_paused || isUnfinishedCompetition(metadata)) &&
        metadata.play_session_id
      ) {
        navigation.clear();
        setRequestedPlaySession({
          id: metadata.play_session_id,
          token: Date.now(),
        });
        setView('play');
      } else {
        await (previous.positionId
          ? navigation.openPosition(previous.positionId)
          : navigation.openMatch(previous.matchId, previous.gameNo));
      }
    })().finally(() => {
      if (!abort.signal.aborted) setRestoring(false);
      if (restorationRequest.current === abort)
        restorationRequest.current = null;
    });
    return () => {
      abort.abort();
      if (restorationRequest.current === abort)
        restorationRequest.current = null;
    };
    // Navigation intent and aborts fence this single startup restore.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  function cancelRestoration() {
    if (!restorationRequest.current) return;
    restorationRequest.current.abort();
    restorationRequest.current = null;
    navigation.cancel();
    setRestoring(false);
  }
  function playModeChanged(session: PlaySession) {
    if (session.play_mode === 'competition' && session.library_match_id) {
      navigation.invalidateMatchAnalysis(session.library_match_id);
    }
  }
  useEffect(() => {
    if (
      navigation.match &&
      navigation.game &&
      !navigation.loading &&
      !navigation.error &&
      navigation.stats
    ) {
      saveSelection({
        matchId: navigation.match.id,
        gameNo: navigation.game.game_no,
        positionId: navigation.position?.id,
      });
    }
  }, [
    navigation.match,
    navigation.game,
    navigation.loading,
    navigation.error,
    navigation.stats,
    navigation.position,
  ]);
  useEffect(() => {
    function move(event: globalThis.KeyboardEvent) {
      if (
        view !== 'analysis' ||
        settingsOpen ||
        event.altKey ||
        event.ctrlKey ||
        event.metaKey
      )
        return;
      const target = event.target as HTMLElement | null;
      if (
        target?.closest(
          'input,textarea,select,button,[contenteditable="true"],[role="tablist"],[role="dialog"]',
        )
      )
        return;
      if (event.key === 'ArrowLeft') {
        event.preventDefault();
        navigation.previous();
      }
      if (event.key === 'ArrowRight') {
        event.preventDefault();
        navigation.next();
      }
    }
    window.addEventListener('keydown', move);
    return () => window.removeEventListener('keydown', move);
  }, [view, settingsOpen, navigation]);
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
  async function upload(files: File[] | FileList | undefined) {
    const selected = Array.from(files ?? []);
    if (!selected.length || uploading) return;
    cancelRestoration();
    setUploading(true);
    setError(undefined);
    setNotice(undefined);
    setImportResults([]);
    const target =
      databaseId && acceptsLibraryMatches(databaseId)
        ? `?database_id=${encodeURIComponent(databaseId)}`
        : '';
    const navigationIntent = navigation.intent();
    const results: {
      name: string;
      status: 'importedFile' | 'duplicate' | 'error';
      detail?: unknown;
    }[] = [];
    for (const file of selected) {
      try {
        if (file.size > 16 * 1024 * 1024) throw new AppError('error.fileSize');
        const body = new FormData();
        body.append('file', file);
        const imported = await request<{ match: Match; duplicate: boolean }>(
          `/imports${target}`,
          { method: 'POST', body },
        );
        setRevision((n) => n + 1);
        results.push({
          name: file.name,
          status: imported.duplicate ? 'duplicate' : 'importedFile',
        });
        if (selected.length === 1 && navigation.intent() === navigationIntent) {
          setView('analysis');
          setDetail('overview');
          void navigation.selectMatch(imported.match);
          setNotice({
            duplicate: imported.duplicate,
            count: imported.match.position_count,
          });
        }
      } catch (e) {
        results.push({ name: file.name, status: 'error', detail: e });
        if (selected.length === 1) setError(e);
      }
      setImportResults([...results]);
    }
    if (selected.length > 1 && navigation.intent() === navigationIntent)
      setView('database');
    setUploading(false);
    if (input.current) input.current.value = '';
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
    cancelRestoration();
    annotationNavigation.current?.abort();
    navigation.clear();
    setView('analysis');
    setDetail('overview');
    setRestoring(false);
    setError(undefined);
    setNotice(undefined);
    setMoveNotes(undefined);
    try {
      localStorage.removeItem('bgtutor.workspace');
    } catch {
      /* Home navigation also works without persistent storage. */
    }
    window.scrollTo(0, 0);
  }
  function openMatch(match: Match, gameNo?: number) {
    cancelRestoration();
    setError(undefined);
    setView('analysis');
    setDetail('overview');
    void (gameNo
      ? navigation.openMatch(match.id, gameNo)
      : navigation.selectMatch(match));
  }
  function searchCurrentPosition() {
    const positionId = navigation.position?.position_id;
    if (!positionId) return;
    cancelRestoration();
    changeDatabase('');
    setRequestedPositionSearch((previous) => ({
      positionId,
      token: (previous?.token ?? 0) + 1,
    }));
    setView('database');
  }
  function searchSimilarPositions() {
    const { position, match } = navigation;
    if (!position || !match) return;
    cancelRestoration();
    setSimilarTarget((previous) => ({
      source: position,
      matchName: match.filename,
      token: (previous?.token ?? 0) + 1,
    }));
    setView('positions');
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
    <section
      className="welcome analysis-welcome"
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        if (!uploading) void upload(event.dataTransfer.files);
      }}
    >
      <span className="eyebrow">BG TUTOR</span>
      <h1>{w('welcomeTitle')}</h1>
      <p>{w('welcomeDescription')}</p>
      <div className="welcome-actions">
        <ActionButton
          className="primary"
          onClick={() => {
            cancelRestoration();
            setView('database');
          }}
          icon="open"
          label={w('openMatch')}
        >
          {w('openMatch')}
        </ActionButton>
        <ActionButton
          onClick={() => input.current?.click()}
          disabled={uploading}
          icon="import"
          label={uploading ? t('app.importRunning') : t('app.importPlain')}
        >
          {uploading ? t('app.importRunning') : t('app.importPlain')}
        </ActionButton>
        <MatchDropZone
          disabled={uploading}
          onFiles={(files) => void upload(files)}
        />
      </div>
      {restoring && <p role="status">{w('lastMatch')}</p>}
    </section>
  );
  return (
    <div className="app-shell">
      <header className="app-header">
        <a
          className="brand"
          href="/"
          aria-label={t('app.home')}
          onClick={goHome}
        >
          <span className="brand-mark">
            b<span>g</span>
          </span>
          <span>BG Tutor</span>
        </a>
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
              tabIndex={view === tab ? 0 : -1}
              ref={(el) => {
                tabs.current[tab] = el;
              }}
              onKeyDown={(event) => tabKey(event, tab)}
              onClick={() => {
                cancelRestoration();
                setView(tab);
              }}
            >
              {tab === 'extras'
                ? benchmarkT('extras')
                : tab === 'play'
                  ? playText('title')
                  : tab === 'learning'
                    ? learnT('study')
                    : tab === 'positions'
                      ? positionText('title')
                      : tab === 'players'
                        ? playerText('title')
                        : w(tab)}
            </button>
          ))}
        </nav>
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
        <input
          ref={input}
          className="sr-only"
          type="file"
          aria-label={t('app.matchFile')}
          accept=".mat,.sgf,.xg,.bgf,.ogxm"
          multiple
          onChange={(event) => void upload(event.target.files ?? undefined)}
        />
      </header>
      {benchmarkActive && (
        <p className="benchmark-global-status" role="status">
          {benchmarkT('globalBusy')}
        </p>
      )}
      <main className={`app-body app-body-${view}`}>
        <section
          className="workspace-panel"
          id="workspace-panel-play"
          role="tabpanel"
          aria-labelledby="workspace-tab-play"
          hidden={view !== 'play'}
        >
          <PlayWorkspace
            visible={view === 'play'}
            theme={theme}
            requestedSession={requestedPlaySession}
            onSaved={() => setRevision((n) => n + 1)}
            onModeChanged={playModeChanged}
            onOpenPausedMatches={() => {
              cancelRestoration();
              changeDatabase(PAUSED_MATCHES_DATABASE_ID);
              setView('database');
              tabs.current.database?.focus();
            }}
          />
        </section>
        {!!(error || navigation.error) && (
          <div className="error" role="alert">
            {formatError(error || navigation.error)}
            <ActionButton
              aria-label={t('app.closeError')}
              onClick={() => {
                setError(undefined);
                navigation.clearError();
              }}
              icon="close"
              label={t('app.closeError')}
              compact
            ></ActionButton>
          </div>
        )}
        {!!settingsError && (
          <div className="error" role="alert">
            {w('configurationError')} {formatError(settingsError)}
            <ActionButton
              onClick={() => setSettingsRevision((value) => value + 1)}
              icon="refresh"
              label={w('retry')}
              compact
            ></ActionButton>
          </div>
        )}
        {notice && (
          <div className="notice" role="status">
            ✓{' '}
            {notice.duplicate
              ? t('app.importDuplicate')
              : t('app.importCount', { count: notice.count })}
            <ActionButton
              aria-label={t('app.closeNotice')}
              onClick={() => setNotice(undefined)}
              icon="close"
              label={t('app.closeNotice')}
              compact
            ></ActionButton>
          </div>
        )}
        {importResults.length > 1 && (
          <div className="import-results" role="status">
            <div className="section-heading">
              <h2>{l('importResults')}</h2>
              <ActionButton
                onClick={() => setImportResults([])}
                aria-label={l('close')}
                icon="close"
                label={l('close')}
                compact
              ></ActionButton>
            </div>
            <ul>
              {importResults.map((result, i) => (
                <li key={i}>
                  {result.name}: {l(result.status)}
                  {result.detail ? ` · ${formatError(result.detail)}` : ''}
                </li>
              ))}
            </ul>
          </div>
        )}
        <div
          id="workspace-panel-analysis"
          role="tabpanel"
          aria-labelledby="workspace-tab-analysis"
          hidden={view !== 'analysis'}
        >
          <AnalysisWorkspace
            active={view === 'analysis'}
            analysisRevision={assessmentRevision}
            metadataRevision={revision}
            bookmarkRevision={bookmarkRevision}
            onBookmarkChanged={bookmarkChanged}
            navigation={navigation}
            jobs={jobs}
            theme={theme}
            defaultEngine={settings?.analysis_engine ?? 'gnubg'}
            defaultPlies={settings?.analysis_plies ?? DEFAULT_ANALYSIS_PLIES}
            analysisInstallationKey={JSON.stringify([
              installedSettings?.version,
              installedSettings?.gnubg_path,
              installedSettings?.analysis_engine,
              settingsRevision,
            ])}
            sourceRevision={bookRevision}
            configured={!!settings && (!configuration || configuration.ready)}
            detail={detail}
            onDetail={setDetail}
            onOpenDatabase={searchCurrentPosition}
            onFindSimilar={searchSimilarPositions}
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
                  reviewPosition: target?.reviewPosition,
                  reviewPlayed: target?.reviewPlayed,
                });
            }}
            deckRevision={deckRevision}
            onDeckChanged={deckChanged}
            onPosition={openPosition}
            scopeRequest={scopeRequest}
            welcome={welcome}
          />
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
            onDatabase={changeDatabase}
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
                    try {
                      localStorage.removeItem('bgtutor.workspace');
                    } catch {
                      /* Storage unavailable. */
                    }
                  }
                });
              }
              if (
                navigation.match &&
                deletedIds.includes(navigation.match.id)
              ) {
                navigation.clear();
                try {
                  localStorage.removeItem('bgtutor.workspace');
                } catch {
                  /* Storage unavailable. */
                }
              }
            }}
            annotationRevision={annotationRevision}
            currentPosition={navigation.position}
            requestedPositionSearch={requestedPositionSearch}
            selectedMatchId={navigation.match?.id}
            onOpen={openMatch}
            onPosition={openPosition}
            onAnnotation={openAnnotation}
            onImport={() => input.current?.click()}
          />
        </div>
        <div
          id="workspace-panel-players"
          role="tabpanel"
          aria-labelledby="workspace-tab-players"
          hidden={view !== 'players'}
        >
          {(view === 'players' || playersOpened) && (
            <PlayerDirectory revision={revision} onOpen={openMatch} />
          )}
        </div>
        <div
          id="workspace-panel-positions"
          role="tabpanel"
          aria-labelledby="workspace-tab-positions"
          hidden={view !== 'positions'}
        >
          <div hidden={!!similarTarget}>
            <PositionBrowser
              visible={view === 'positions' && !similarTarget}
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
            />
          </div>
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
        </div>
        <div
          id="workspace-panel-learning"
          role="tabpanel"
          aria-labelledby="workspace-tab-learning"
          hidden={view !== 'learning'}
        >
          <StudyWorkspace
            visible={view === 'learning'}
            requestedDeck={requestedDeck}
            theme={theme}
            revision={deckRevision}
            onChanged={deckChanged}
            onStudyDeck={openStudyDeck}
          />
        </div>
        <div
          id="workspace-panel-extras"
          role="tabpanel"
          aria-labelledby="workspace-tab-extras"
          hidden={view !== 'extras'}
        >
          <ExtrasWorkspace
            visible={view === 'extras'}
            tool={extrasTool}
            setTool={setExtrasTool}
            defaultPlies={settings?.analysis_plies ?? DEFAULT_ANALYSIS_PLIES}
            playPlies={settings?.play_plies}
          />
        </div>
        <footer className="app-footer">
          <span>
            <span className={`status-dot ${health === 'ok' ? 'ready' : ''}`} />
            {health === 'ok'
              ? t('app.engineConnected')
              : health === 'checking'
                ? t('app.healthChecking')
                : t('app.healthUnavailable')}
          </span>
          <a href="/api/openapi.json" target="_blank" rel="noreferrer">
            OpenAPI ↗
          </a>
        </footer>
      </main>
      {moveNotes && (
        <MoveNotesDialog
          target={moveNotes}
          theme={theme}
          onClose={() => setMoveNotes(undefined)}
          onSaved={notesSaved}
        />
      )}
      <SettingsDialog
        initialCategory={settingsCategory}
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onBookSaved={() => setBookRevision((n) => n + 1)}
        onBenchmark={() => {
          cancelRestoration();
          setSettingsOpen(false);
          setExtrasTool('benchmark');
          setView('extras');
        }}
        theme={theme}
        appearance={appearance}
        onApply={(applied) => {
          settingsLoad.current?.abort();
          setTheme(applied.theme);
          setLocale(applied.locale);
          applyAppearance(applied.appearance);
          setInstalledSettings(applied.settings);
          setSettingsError(undefined);
        }}
      />
    </div>
  );
}
