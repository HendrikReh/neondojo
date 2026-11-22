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
import { loadBenchmarkStatus } from '../benchmark-api';
import ExtrasWorkspace, {
  readExtrasTool,
  saveExtrasTool,
} from '../ExtrasWorkspace';
import { useBenchmarkText } from '../benchmark-messages';
import ActionButton from '../shared/ui/ActionButton';
import PlayWorkspace from '../workspaces/play/PlayWorkspace';
import MatchDropZone from '../MatchDropZone';
import { usePlayText } from '../workspaces/play/play-messages';
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from 'react';
import {
  request,
  terminal,
  type Job,
  type Match,
  type Position,
} from '../shared/api/api';
import AnalysisWorkspace, {
  type AnalysisDetail,
} from '../workspaces/analysis/AnalysisWorkspace';
import PlayerDirectory from '../PlayerDirectory';
import { usePlayerText } from '../player-directory-messages';
import MatchDatabase, { type PositionSearchRequest } from '../MatchDatabase';
import MatchAnalysisDialog from '../workspaces/analysis/match/MatchAnalysisDialog';
import BulkMatchAnalysisDialog from '../workspaces/analysis/match/BulkMatchAnalysisDialog';
import StudyWorkspace from '../workspaces/study/StudyWorkspace';
import type { AssessmentEntry } from '../workspaces/study/reports/match-assessment-api';
import PositionBrowser from '../workspaces/positions/PositionBrowser';
import SimilarPositions from '../workspaces/positions/SimilarPositions';
import { usePositionText } from '../workspaces/positions/position-browser-messages';
import SettingsDialog from '../settings/SettingsDialog';
import { loadSettings, type AppSettings } from '../settings/settings-api';
import { readTheme, saveTheme } from '../settings/ui-preferences';
import type { Theme } from '../shared/board/themes';
import { useGameNavigation } from './useGameNavigation';
import { useJobs } from '../shared/jobs/useJobs';
import { useBackgroundJobs } from './useBackgroundJobs';
import { useI18n } from '../shared/i18n/i18n';
import { useWorkspaceText } from '../shared/i18n/workspace-messages';
import { useLearningText } from '../workspaces/study/flashcards/learning-messages';
import {
  decodeBulkAnalysisSession,
  readSelection,
  clearSelection,
  saveSelection,
} from './workspace-state';
import { AppError } from '../shared/i18n/i18n-core';
import type { AnnotationHit } from '../workspaces/analysis/notes/AnnotationSearch';
import type { AnnotationKind } from '../workspaces/analysis/notes/annotation-api';
import './workspace.css';
import {
  acceptsLibraryMatches,
  PAUSED_MATCHES_DATABASE_ID,
  readDatabase,
  type StoredMove,
  type LibraryMatch,
} from '../library-api';
import { useLibraryText, type ImportWarning } from '../library-messages';
import { ImportWarningList } from '../ImportWarningList';
import ImportRunPanel, {
  type ImportResult,
  type ImportRun,
} from '../ImportRunPanel';
import {
  MATCH_FILE_ACCEPT,
  matchFileEntries,
  readDroppedMatchFiles,
  selectFolderMatchFiles,
  type MatchFileEntry,
} from '../match-import';

import { isUnfinishedCompetition } from '../library-match-summary';
import type { PlaySession } from '../workspaces/play/play-api';
import { DEFAULT_ANALYSIS_PLIES } from '../shared/engine/analysis-depth';
import MoveNotesDialog, {
  type MoveNotesTarget,
} from '../workspaces/analysis/notes/MoveNotesDialog';
import { useUiAppearance } from '../settings/appearance/useUiAppearance';
import { useButtonShortcuts } from './useButtonShortcuts';
import DecisionSearch from '../workspaces/positions/DecisionSearch';
import { useDecisionSearchText } from '../workspaces/positions/decision-search-messages';

type ImportSource = {
  files: readonly MatchFileEntry[];
  /** Set for a folder, with the number of files left out. */
  folder?: { skipped: number };
};

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
  const decisionText = useDecisionSearchText();
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
  const [importRun, setImportRun] = useState<ImportRun>();
  const importStop = useRef(false);
  const importBusy = useRef(false);
  const annotationNavigation = useRef<AbortController | undefined>(undefined);
  useEffect(() => () => annotationNavigation.current?.abort(), []);
  const [bulkAnalysis, setBulkAnalysis] = useWorkspaceSession(
    'library.bulkAnalysis',
    undefined,
    { jobs: {}, showMatchList: false },
    decodeBulkAnalysisSession,
  );
  const bulkJobIds = Object.keys(bulkAnalysis.jobs);
  const [view, setView] = useState<View>(() =>
    bulkAnalysis.showMatchList && bulkJobIds.length ? 'database' : 'analysis',
  );
  useEffect(() => {
    if (view === 'players') setPlayersOpened(true);
  }, [view]);
  const [detail, setDetail] = useState<AnalysisDetail>('overview');
  const [localTheme, setTheme] = useState<Theme>(readTheme);
  const theme = confirmed?.appearance.board_theme ?? localTheme;
  const [installedSettings, setInstalledSettings] = useState<AppSettings>();
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
  const [settingsError, setSettingsError] = useState<unknown>();
  const [settingsRevision, setSettingsRevision] = useState(0);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [bookRevision, setBookRevision] = useState(0);
  const bookConfigurationKey = openingBookConfigurationKey(
    confirmed?.opening_book,
  );
  const [health, setHealth] = useState<'checking' | 'ok' | 'degraded'>(
    'checking',
  );
  const [error, setError] = useState<unknown>();
  const [notice, setNotice] = useState<{
    duplicate: boolean;
    count: number;
    warnings: ImportWarning[];
  }>();
  const [uploading, setUploading] = useState(false);
  const [revision, setRevision] = useState(0);
  const [annotationRevision, setAnnotationRevision] = useState(0);
  const [deckRevision, setDeckRevision] = useState(0);
  const [bookmarkRevision, setBookmarkRevision] = useState(0);
  const [analysisSelection, setAnalysisSelection] = useState<
    | {
        kind: 'single';
        match: Match & Pick<LibraryMatch, 'display_name'>;
      }
    | { kind: 'bulk'; matches: LibraryMatch[]; unavailableCount: number }
  >();
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
  const [restoring, setRestoring] = useState(false);
  const restorationRequest = useRef<AbortController | null>(null);
  const navigation = useGameNavigation();
  const {
    active: runningAnalysisJobs,
    tracked: trackedAnalysisJobs,
    track,
    missing: missingAnalysisJobs,
    assessmentRevision,
  } = useBackgroundJobs(bulkJobIds);
  const visibleBulkJobIds = databaseId
    ? bulkJobIds.filter((id) => bulkAnalysis.jobs[id] === databaseId)
    : bulkJobIds;
  const bulkAnalysisProgress = {
    total: visibleBulkJobIds.length,
    completed: 0,
    failed: 0,
    missing: 0,
    loading: false,
  };
  for (const id of visibleBulkJobIds) {
    const status = trackedAnalysisJobs[id]?.status;
    if (missingAnalysisJobs.has(id)) bulkAnalysisProgress.missing++;
    else if (status === 'completed') bulkAnalysisProgress.completed++;
    else if (status === 'failed') bulkAnalysisProgress.failed++;
    else if (!status) bulkAnalysisProgress.loading = true;
  }
  const bulkJobPending = (id: string) => {
    const job = trackedAnalysisJobs[id];
    return !missingAnalysisJobs.has(id) && (!job || !terminal(job));
  };
  const bulkAnalysisInProgress = bulkJobIds.some(bulkJobPending);
  const reopenMatchList = view === 'database' && bulkAnalysisInProgress;
  useEffect(() => {
    if (bulkAnalysis.showMatchList !== reopenMatchList)
      setBulkAnalysis((previous) => ({
        ...previous,
        showMatchList: reopenMatchList,
      }));
  }, [bulkAnalysis.showMatchList, reopenMatchList, setBulkAnalysis]);
  const analysisInstallationKey = JSON.stringify([
    installedSettings?.version,
    installedSettings?.gnubg_path,
    installedSettings?.analysis_engine,
    settingsRevision,
  ]);
  const jobs = useJobs(navigation.position?.id, analysisEngine, track);
  const settingsLoad = useRef<AbortController | undefined>(undefined);
  const input = useRef<HTMLInputElement>(null);
  const folderInput = useRef<HTMLInputElement>(null);
  const importDestination = useRef<string | undefined>(undefined);
  const tabs = useRef<Partial<Record<View, HTMLButtonElement | null>>>({});
  useEffect(() => {
    const chooser = input.current;
    const clearDestination = () => {
      importDestination.current = undefined;
    };
    const folderChooser = folderInput.current;
    // React has no typed prop for the directory picker attribute.
    if (folderChooser) folderChooser.webkitdirectory = true;
    chooser?.addEventListener('cancel', clearDestination);
    folderChooser?.addEventListener('cancel', clearDestination);
    return () => {
      chooser?.removeEventListener('cancel', clearDestination);
      folderChooser?.removeEventListener('cancel', clearDestination);
    };
  }, []);
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
      let missing = false;
      try {
        metadata = await request<LibraryMatch>(
          `/matches/${previous.matchId}/metadata`,
          { signal: abort.signal },
        );
      } catch (e) {
        // Old/missing metadata still uses normal navigation and its real error.
        missing = e instanceof AppError && String(e.params.status) === '404';
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
        setView((currentView) =>
          currentView === 'analysis' ? 'play' : currentView,
        );
      } else {
        const opening = previous.positionId
          ? navigation.openPosition(previous.positionId)
          : navigation.openMatch(previous.matchId, previous.gameNo);
        // Opening starts a new navigation; a later one must not be cleared.
        const openIntent = navigation.intent();
        const opened = await opening;
        if (
          !opened &&
          missing &&
          !abort.signal.aborted &&
          navigation.intent() === openIntent
        ) {
          // The last match or position no longer exists: start empty.
          navigation.clear();
          clearSelection();
        }
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
      navigation.gameLoaded
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
    navigation.gameLoaded,
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
  function chooseImportFiles(destination?: string) {
    importDestination.current = destination;
    input.current?.click();
  }
  function chooseImportFolder(destination: string) {
    importDestination.current = destination;
    folderInput.current?.click();
  }
  /**
   * Imports files one by one. `source` may still be reading a dropped folder;
   * the import counts as running from the call on, so a second drop or pick
   * waits for this one.
   */
  async function upload(
    source: ImportSource | Promise<ImportSource>,
    destination = databaseId,
  ) {
    if (importBusy.current) return;
    importBusy.current = true;
    setUploading(true);
    const navigationIntent = navigation.intent();
    try {
      let files: readonly MatchFileEntry[];
      let folder: ImportSource['folder'];
      try {
        ({ files, folder } = await source);
      } catch (e) {
        setError(e);
        return;
      }
      await importFiles(files, destination, folder, navigationIntent);
    } finally {
      importBusy.current = false;
      setUploading(false);
      if (input.current) input.current.value = '';
    }
  }
  async function importFiles(
    selected: readonly MatchFileEntry[],
    destination: string,
    folder: ImportSource['folder'],
    navigationIntent: ReturnType<typeof navigation.intent>,
  ) {
    if (!selected.length && !folder) return;
    cancelRestoration();
    setError(undefined);
    setNotice(undefined);
    importStop.current = false;
    // A folder run always reports in the results panel, even for one file.
    const batch = !!folder || selected.length > 1;
    const run: ImportRun = {
      results: [],
      total: selected.length,
      skipped: folder?.skipped ?? 0,
      folder: !!folder,
      running: selected.length > 0,
      stopping: false,
      stopped: false,
    };
    setImportRun(batch ? run : undefined);
    if (!selected.length) return;
    const target =
      destination && acceptsLibraryMatches(destination)
        ? `?database_id=${encodeURIComponent(destination)}`
        : '';
    const results: ImportResult[] = [];
    // Large batches refresh the library at most every two seconds instead of
    // reloading the list after every file.
    let refreshedAt = 0;
    let unrefreshed = false;
    for (const { file, path: name } of selected) {
      if (importStop.current) break;
      try {
        if (file.size > 16 * 1024 * 1024) throw new AppError('error.fileSize');
        const body = new FormData();
        body.append('file', file);
        const imported = await request<{
          match: Match;
          duplicate: boolean;
          warnings: ImportWarning[];
        }>(`/imports${target}`, { method: 'POST', body });
        unrefreshed = true;
        if (!batch || Date.now() - refreshedAt >= 2000) {
          setRevision((n) => n + 1);
          refreshedAt = Date.now();
          unrefreshed = false;
        }
        results.push({
          name,
          status: imported.duplicate ? 'duplicate' : 'importedFile',
          warnings: imported.warnings,
        });
        if (!batch && navigation.intent() === navigationIntent) {
          setView('analysis');
          setDetail('overview');
          void navigation.selectMatch(imported.match);
          setNotice({
            duplicate: imported.duplicate,
            count: imported.match.position_count,
            warnings: imported.warnings,
          });
        }
      } catch (e) {
        results.push({ name, status: 'error', detail: e });
        if (!batch) setError(e);
      }
      if (batch)
        setImportRun(
          (current) => current && { ...current, results: [...results] },
        );
    }
    if (unrefreshed) setRevision((n) => n + 1);
    if (batch) {
      const stopped = results.length < selected.length;
      setImportRun(
        (current) =>
          current && {
            ...current,
            results: [...results],
            running: false,
            stopping: false,
            stopped,
          },
      );
      if (navigation.intent() === navigationIntent) setView('database');
    }
  }
  /** Reads a drop now, while its data is still available. */
  function dropImport(transfer: DataTransfer) {
    if (importBusy.current) return;
    void upload(
      readDroppedMatchFiles(transfer).then(({ files, folder }) => {
        if (!folder) return { files };
        const selected = selectFolderMatchFiles(files);
        return {
          files: selected.files,
          folder: { skipped: selected.skipped },
        };
      }),
    );
  }
  function stopImport() {
    importStop.current = true;
    setImportRun((current) => current && { ...current, stopping: true });
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
    clearSelection();
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
    <section
      className="welcome analysis-welcome"
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => {
        event.preventDefault();
        dropImport(event.dataTransfer);
      }}
    >
      <span className="eyebrow">NEONDOJO</span>
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
          onClick={() => chooseImportFiles()}
          disabled={uploading}
          icon="import"
          label={uploading ? t('app.importRunning') : t('app.importPlain')}
        >
          {uploading ? t('app.importRunning') : t('app.importPlain')}
        </ActionButton>
        <MatchDropZone disabled={uploading} onDrop={dropImport} />
      </div>
      {restoring && <p role="status">{w('lastMatch')}</p>}
    </section>
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
  const analysisDialogs = (
    <>
      {analysisSelection?.kind === 'single' && (
        <MatchAnalysisDialog
          key={analysisSelection.match.id}
          match={analysisSelection.match}
          defaultEngine={analysisEngine}
          defaultPlies={analysisPlies}
          configured
          analysisInstallationKey={analysisInstallationKey}
          trackedJobs={trackedAnalysisJobs}
          onAccepted={(job) => {
            track(job);
            setRevision((value) => value + 1);
          }}
          onShowResults={(job) => {
            setAnalysisSelection(undefined);
            openMatch(analysisSelection.match, undefined, job);
            requestAnimationFrame(() => tabs.current.analysis?.focus());
          }}
          onClose={() => setAnalysisSelection(undefined)}
        />
      )}
      {analysisSelection?.kind === 'bulk' && (
        <BulkMatchAnalysisDialog
          matches={analysisSelection.matches}
          unavailableCount={analysisSelection.unavailableCount}
          defaultEngine={analysisEngine}
          defaultPlies={analysisPlies}
          configured
          analysisInstallationKey={analysisInstallationKey}
          onAccepted={(jobs, continuesBatch) => {
            if (!jobs.length) return;
            const databaseByMatch = new Map(
              analysisSelection.matches.map((match) => [
                match.id,
                match.database_id,
              ]),
            );
            for (const job of jobs) track(job);
            setBulkAnalysis((previous) => {
              const overlaps = Object.keys(previous.jobs).some(bulkJobPending);
              const tracked =
                continuesBatch || overlaps ? { ...previous.jobs } : {};
              for (const job of jobs) {
                tracked[job.id] =
                  databaseByMatch.get(job.match_uuid ?? '') ?? null;
              }
              return { jobs: tracked, showMatchList: true };
            });
            setRevision((value) => value + 1);
          }}
          onClose={() => setAnalysisSelection(undefined)}
        />
      )}
    </>
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
          <img
            className="brand-mark"
            src="/neondojo-logo.svg"
            alt=""
            width={30}
            height={30}
          />
          <span>NeonDojo</span>
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
          accept={MATCH_FILE_ACCEPT}
          multiple
          onChange={(event) => {
            const destination = importDestination.current;
            importDestination.current = undefined;
            void upload(
              { files: matchFileEntries(event.target.files ?? []) },
              destination,
            );
          }}
        />
        <input
          ref={folderInput}
          className="sr-only"
          type="file"
          aria-label={l('matchFolder')}
          multiple
          onChange={(event) => {
            const destination = importDestination.current;
            importDestination.current = undefined;
            const { files, skipped } = selectFolderMatchFiles(
              matchFileEntries(event.target.files ?? []),
            );
            // Choosing the same folder again must fire another change.
            event.target.value = '';
            void upload({ files, folder: { skipped } }, destination);
          }}
        />
      </header>
      {benchmarkActive && (
        <p className="benchmark-global-status" role="status">
          {benchmarkT('globalBusy')}
        </p>
      )}
      <main className={`app-body app-body-${view}`}>
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
            <ImportWarningList warnings={notice.warnings} />
          </div>
        )}
        {importRun && (
          <ImportRunPanel
            run={importRun}
            onStop={stopImport}
            onClose={() => setImportRun(undefined)}
          />
        )}
        {/* Global errors, notices and import progress stay above every workspace. */}
        <section
          className="workspace-panel"
          id="workspace-panel-play"
          role="tabpanel"
          aria-labelledby="workspace-tab-play"
          hidden={view !== 'play'}
        >
          <PlayWorkspace
            visible={view === 'play'}
            settings={settings}
            theme={theme}
            requestedSession={requestedPlaySession}
            onSaved={() => setRevision((n) => n + 1)}
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
        </section>
        <div
          id="workspace-panel-analysis"
          role="tabpanel"
          aria-labelledby="workspace-tab-analysis"
          hidden={view !== 'analysis'}
        >
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
                .filter((job) => job.kind === 'match' && job.game_no == null)
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
            uploading={uploading}
            onImport={chooseImportFiles}
            onImportFolder={chooseImportFolder}
          />
          {view === 'database' && analysisDialogs}
        </div>
        <div
          id="workspace-panel-players"
          role="tabpanel"
          aria-labelledby="workspace-tab-players"
          hidden={view !== 'players'}
        >
          {(view === 'players' || playersOpened) && (
            <PlayerDirectory
              revision={revision}
              onOpen={openMatch}
              onAnalyzeMatches={selectForAnalysis}
            />
          )}
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
            aria-label={decisionText('scope')}
            hidden={!!similarTarget}
          >
            {(['saved', 'all'] as const).map((scope) => (
              <button
                key={scope}
                type="button"
                aria-pressed={positionScope === scope}
                onClick={() => setPositionScope(scope)}
              >
                {decisionText(scope)}
              </button>
            ))}
          </div>
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
                  void navigation.openMatch(decision.matchId, decision.gameNo);
                }}
              />
            </div>
          )}
          <div hidden={!!similarTarget || positionScope === 'all'}>
            <PositionBrowser
              visible={
                view === 'positions' &&
                !similarTarget &&
                positionScope === 'saved'
              }
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
            requestedAssessment={requestedAssessment}
            theme={theme}
            revision={deckRevision}
            onChanged={deckChanged}
            onStudyDeck={openStudyDeck}
            onOpenPlayers={() => setView('players')}
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
            onOpenMatch={openMatch}
            defaultPlies={analysisPlies}
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
          settingsLoad.current?.abort();
          setTheme(applied.theme);
          setLocale(applied.locale);
          applyAppearance(applied.appearance);
          setInstalledSettings(applied.settings);
          setSettingsError(undefined);
          if (applied.myProfileChanged) setRevision((n) => n + 1);
        }}
      />
    </div>
  );
}
