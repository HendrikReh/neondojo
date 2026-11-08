import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from 'react';
import { request, type Match, type Position } from './api';
import AnalysisWorkspace, { type AnalysisDetail } from './AnalysisWorkspace';
import MatchDatabase from './MatchDatabase';
import LearningWorkspace from './LearningPanel';
import PositionBrowser from './PositionBrowser';
import { usePositionText } from './position-browser-messages';
import SettingsDialog from './SettingsDialog';
import { loadSettings, type AppSettings } from './settings-api';
import { readTheme, saveTheme } from './ui-preferences';
import type { Theme } from './themes';
import { useGameNavigation } from './useGameNavigation';
import { useJobs } from './useJobs';
import { useBackgroundJobs } from './useBackgroundJobs';
import { useI18n, type Locale } from './i18n';
import { useWorkspaceText } from './workspace-messages';
import { useLearningText } from './learning-messages';
import { readSelection, saveSelection } from './workspace-state';
import { AppError } from './i18n-core';
import type { AnnotationHit } from './AnnotationSearch';
import type { AnnotationKind } from './annotation-api';
import './workspace.css';
import OpeningBookDialog from './OpeningBookDialog';
import {
  readDatabase,
  saveDatabase,
  defaultDatabaseId,
  type StoredMove,
} from './library-api';
import { useLibraryText } from './library-messages';
import { DEFAULT_ANALYSIS_PLIES } from './analysis-depth';
import MoveNotesDialog, { type MoveNotesTarget } from './MoveNotesDialog';

type View = 'analysis' | 'database' | 'positions' | 'learning';
const views: View[] = ['analysis', 'database', 'positions', 'learning'];
export default function App() {
  const { t, setLocale, formatError } = useI18n();
  const w = useWorkspaceText(),
    learnT = useLearningText();
  const l = useLibraryText();
  const positionText = usePositionText();
  const [requestedDeck, setRequestedDeck] = useState<{
    id: string;
    token: number;
  }>();
  const [databaseId, setDatabaseId] = useState(readDatabase);
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
    if (databaseId) saveDatabase(databaseId);
  }, [databaseId]);
  useEffect(() => () => annotationNavigation.current?.abort(), []);
  const [view, setView] = useState<View>('analysis');
  const [detail, setDetail] = useState<AnalysisDetail>('overview');
  const [theme, setTheme] = useState<Theme>(readTheme);
  const [settings, setSettings] = useState<AppSettings>();
  const [settingsError, setSettingsError] = useState<unknown>();
  const [settingsRevision, setSettingsRevision] = useState(0);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [bookOpen, setBookOpen] = useState(false);
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
  const [scopeRequest, setScopeRequest] = useState<{
    kind: AnnotationKind;
    token: number;
  }>();
  const [restoring, setRestoring] = useState(false);
  const navigation = useGameNavigation();
  const { track, assessmentRevision } = useBackgroundJobs();
  const jobs = useJobs(navigation.position?.id, track);
  const settingsLoad = useRef<AbortController | undefined>(undefined);
  const input = useRef<HTMLInputElement>(null);
  const tabs = useRef<Partial<Record<View, HTMLButtonElement | null>>>({});
  useEffect(() => saveTheme(theme), [theme]);
  useEffect(() => {
    Object.values(jobs.jobs).forEach(track);
  }, [jobs.jobs, track]);
  useEffect(() => {
    const abort = new AbortController();
    settingsLoad.current = abort;
    void loadSettings(abort.signal)
      .then((value) => {
        if (abort.signal.aborted) return;
        setSettings((current) =>
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
    let alive = true;
    setRestoring(true);
    const restored = previous.positionId
      ? navigation.openPosition(previous.positionId)
      : navigation.openMatch(previous.matchId, previous.gameNo);
    void restored.finally(() => {
      if (alive) setRestoring(false);
    });
    return () => {
      alive = false;
    };
    // The navigation hook fences restoration against subsequent user actions.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
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
        bookOpen ||
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
  }, [view, settingsOpen, bookOpen, navigation]);
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
    setView(views[next]);
    tabs.current[views[next]]?.focus();
  }
  async function upload(files: File[] | FileList | undefined) {
    const selected = Array.from(files ?? []);
    if (!selected.length || uploading) return;
    setUploading(true);
    setError(undefined);
    setNotice(undefined);
    setImportResults([]);
    const target = databaseId || defaultDatabaseId;
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
          `/imports?database_id=${target}`,
          { method: 'POST', body },
        );
        setRevision((n) => n + 1);
        results.push({
          name: file.name,
          status: imported.duplicate ? 'duplicate' : 'importedFile',
        });
        if (selected.length === 1) {
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
    if (selected.length > 1) setView('database');
    setUploading(false);
    if (input.current) input.current.value = '';
  }
  function openMatch(match: Match, gameNo?: number) {
    setError(undefined);
    setView('analysis');
    setDetail('overview');
    void (gameNo
      ? navigation.openMatch(match.id, gameNo)
      : navigation.selectMatch(match));
  }
  function openPosition(position: Position | string) {
    setView('analysis');
    void navigation.openPosition(position);
  }
  useEffect(() => {
    if (view !== 'analysis' || settingsOpen)
      annotationNavigation.current?.abort();
  }, [view, settingsOpen]);
  function openAnnotation(hit: AnnotationHit) {
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
        <button className="primary" onClick={() => setView('database')}>
          {w('openMatch')}
        </button>
        <button onClick={() => input.current?.click()} disabled={uploading}>
          {uploading ? t('app.importRunning') : t('app.import')}
        </button>
      </div>
      {restoring && <p role="status">{w('lastMatch')}</p>}
      <small>{t('app.dropFile')}</small>
    </section>
  );
  return (
    <div className="app-shell">
      <header className="app-header">
        <a className="brand" href="/" aria-label={t('app.home')}>
          <span className="brand-mark">
            b<span>g</span>
          </span>
          <span>BG Tutor</span>
        </a>
        <nav className="app-tabs" role="tablist" aria-label={w('navigation')}>
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
              onClick={() => setView(tab)}
            >
              {tab === 'learning'
                ? learnT('study')
                : tab === 'positions'
                  ? positionText('title')
                  : w(tab)}
            </button>
          ))}
        </nav>
        <div className="app-header-actions">
          <button
            className="settings-trigger"
            onClick={() => setSettingsOpen(true)}
          >
            <span aria-hidden="true">⚙</span>
            {w('settings')}
          </button>
        </div>
        <input
          ref={input}
          className="sr-only"
          type="file"
          aria-label={t('app.matchFile')}
          accept=".mat,.sgf,.xg,.bgf"
          multiple
          onChange={(event) => void upload(event.target.files ?? undefined)}
        />
      </header>
      <main
        className={`app-body${view === 'analysis' ? ' app-body-analysis' : ''}`}
      >
        {!!(error || navigation.error) && (
          <div className="error" role="alert">
            {formatError(error || navigation.error)}
            <button
              aria-label={t('app.closeError')}
              onClick={() => {
                setError(undefined);
                navigation.clearError();
              }}
            >
              ×
            </button>
          </div>
        )}
        {!!settingsError && (
          <div className="error" role="alert">
            {w('configurationError')} {formatError(settingsError)}
            <button onClick={() => setSettingsRevision((value) => value + 1)}>
              {w('retry')}
            </button>
          </div>
        )}
        {notice && (
          <div className="notice" role="status">
            ✓{' '}
            {notice.duplicate
              ? t('app.importDuplicate')
              : t('app.importCount', { count: notice.count })}
            <button
              aria-label={t('app.closeNotice')}
              onClick={() => setNotice(undefined)}
            >
              ×
            </button>
          </div>
        )}
        {importResults.length > 1 && (
          <div className="import-results" role="status">
            <div className="section-heading">
              <h2>{l('importResults')}</h2>
              <button
                onClick={() => setImportResults([])}
                aria-label={l('close')}
              >
                ×
              </button>
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
            analysisRevision={assessmentRevision}
            navigation={navigation}
            jobs={jobs}
            theme={theme}
            defaultPlies={settings?.analysis_plies ?? DEFAULT_ANALYSIS_PLIES}
            sourceRevision={bookRevision}
            onBookSettings={() => setBookOpen(true)}
            configured={!!settings}
            detail={detail}
            onDetail={setDetail}
            onOpenDatabase={() => setView('database')}
            onTrackJob={track}
            onNotesSaved={notesSaved}
            notesSuspended={!!moveNotes || view !== 'analysis'}
            onMoveNotes={() => {
              if (navigation.match && navigation.game)
                setMoveNotes({
                  match: navigation.match,
                  gameNo: navigation.game.game_no,
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
            revision={revision}
            databaseId={databaseId}
            onDatabase={setDatabaseId}
            theme={theme}
            visible={view === 'database'}
            onMoveNotes={setMoveNotes}
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
            selectedMatchId={navigation.match?.id}
            onOpen={openMatch}
            onPosition={openPosition}
            onAnnotation={openAnnotation}
            onImport={() => input.current?.click()}
          />
        </div>
        <div
          id="workspace-panel-positions"
          role="tabpanel"
          aria-labelledby="workspace-tab-positions"
          hidden={view !== 'positions'}
        >
          <PositionBrowser
            visible={view === 'positions'}
            revision={revision}
            annotationRevision={annotationRevision}
            deckRevision={deckRevision}
            theme={theme}
            onOpenPosition={(id) => {
              setDetail('overview');
              openPosition(id);
            }}
            onDeckChanged={deckChanged}
            onStudy={(id) => {
              setRequestedDeck((old) => ({ id, token: (old?.token ?? 0) + 1 }));
              setView('learning');
            }}
          />
        </div>
        <div
          id="workspace-panel-learning"
          role="tabpanel"
          aria-labelledby="workspace-tab-learning"
          hidden={view !== 'learning'}
        >
          <LearningWorkspace
            requestedDeck={requestedDeck}
            theme={theme}
            revision={deckRevision}
            onChanged={deckChanged}
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
      {bookOpen && (
        <OpeningBookDialog
          onClose={() => setBookOpen(false)}
          onSaved={() => setBookRevision((n) => n + 1)}
        />
      )}
      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onBookSettings={() => setBookOpen(true)}
        theme={theme}
        onApply={(applied: {
          theme: Theme;
          locale: Locale;
          settings: AppSettings;
        }) => {
          settingsLoad.current?.abort();
          setTheme(applied.theme);
          setLocale(applied.locale);
          setSettings(applied.settings);
          setSettingsError(undefined);
        }}
      />
    </div>
  );
}
