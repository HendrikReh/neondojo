import { lazy, type ReactNode } from 'react';
import type { Job } from '../shared/http/api';
import type { Theme } from '../shared/board/themes';
import type { useJobs } from '../shared/jobs/useJobs';
import { useWorkspaceText } from '../shared/i18n/workspace-messages';
import type { AppSettings } from '../settings/settings-api';
import type { MatchAnalysisEngine } from '../workspaces/analysis/match/useMatchAnalysis';
import type { LibraryMatch } from '../workspaces/library/library-api';
import MatchDatabase from '../workspaces/library/MatchDatabase';
import type { useBenchmarkActivity } from '../workspaces/extras/useBenchmarkActivity';
import type { useMatchImporter } from './useMatchImporter';
import type { useAppNavigation } from './useAppNavigation';
import type { AnalysisSelection } from './AnalysisDialogs';
import LazyPanel, { MountHidden } from './LazyPanel';

const HomePage = lazy(() => import('./HomePage'));
const AnalysisWorkspace = lazy(
  () => import('../workspaces/analysis/AnalysisWorkspace'),
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
const MoveNotesDialog = lazy(
  () => import('../workspaces/analysis/notes/MoveNotesDialog'),
);

export default function AppWorkspacePanels({
  nav,
  theme,
  settings,
  importer,
  jobs,
  runningAnalysisJobs,
  assessmentRevision,
  analysisRevision,
  analysisEngine,
  analysisPlies,
  analysisInstallationKey,
  bulkAnalysisProgress,
  benchmarkActivity,
  welcome,
  analysisDialogs,
  onSelectForAnalysis,
  onSetAnalysisSelection,
  onTrackJob,
  onOpenPlaySettings,
}: {
  nav: ReturnType<typeof useAppNavigation>;
  theme: Theme;
  settings?: AppSettings;
  importer: ReturnType<typeof useMatchImporter>;
  jobs: ReturnType<typeof useJobs>;
  runningAnalysisJobs: Job[];
  assessmentRevision?: string;
  analysisRevision: string;
  analysisEngine: MatchAnalysisEngine;
  analysisPlies: number;
  analysisInstallationKey: string;
  bulkAnalysisProgress: {
    total: number;
    completed: number;
    failed: number;
    missing: number;
    loading: boolean;
  };
  benchmarkActivity: ReturnType<typeof useBenchmarkActivity>;
  welcome: ReactNode;
  analysisDialogs: ReactNode;
  onSelectForAnalysis: (
    matches: LibraryMatch[],
    unavailableCount: number,
  ) => void;
  onSetAnalysisSelection: (selection: AnalysisSelection) => void;
  onTrackJob: (job: Job | undefined) => void;
  onOpenPlaySettings: () => void;
}) {
  const w = useWorkspaceText();
  const { view, navigation } = nav;

  return (
    <>
      {view === 'home' && (
        <LazyPanel>
          <HomePage
            match={navigation.match}
            position={navigation.gameLoaded ? navigation.position : undefined}
            theme={theme}
            loading={nav.restoring || navigation.loading}
            importer={importer}
            restoredPlayId={nav.restoredPlayId}
            onContinue={() => {
              nav.setDetail('overview');
              nav.setView('analysis');
            }}
            onOpenLibrary={() => {
              nav.cancelRestoration();
              nav.setView('database');
            }}
            onPlay={() => {
              nav.cancelRestoration();
              nav.requestPlaySetup();
              nav.setView('play');
            }}
            onResumePlay={(session) =>
              nav.openPlaySession(session.id, session.paused, session.version)
            }
            onSearch={() => {
              nav.cancelRestoration();
              nav.setSimilarTarget(undefined);
              nav.setPositionScope('all');
              nav.setView('positions');
            }}
            onStudy={nav.openStudyDeck}
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
          {nav.opened('play') && (
            <MountHidden visible={view === 'play'}>
              {(visible) => (
                <PlayWorkspace
                  visible={visible}
                  settings={settings}
                  theme={theme}
                  requestedSession={nav.requestedPlaySession}
                  setupRequest={nav.playSetupRequest}
                  onSaved={() => {
                    nav.setRestoredPlayId(undefined);
                    nav.bumpRevision();
                  }}
                  onModeChanged={nav.playModeChanged}
                  onOpenPlaySettings={onOpenPlaySettings}
                  onOpenAssessment={nav.openPlayAssessment}
                  onOpenPausedMatches={nav.openPausedMatches}
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
        {nav.analysisOpened ? (
          <LazyPanel>
            <AnalysisWorkspace
              active={view === 'analysis'}
              requestedAnalysis={nav.requestedAnalysis}
              requestedCubeDecision={nav.requestedCubeDecision}
              navigation={navigation}
              jobs={jobs}
              runningJobs={runningAnalysisJobs}
              theme={theme}
              revisions={{
                openingBook: nav.bookRevision,
                analysis: analysisRevision,
                metadata: nav.revision,
                bookmark: nav.bookmarkRevision,
                deck: nav.deckRevision,
              }}
              engine={{
                defaultEngine: analysisEngine,
                defaultPlies: analysisPlies,
                installationKey: analysisInstallationKey,
                configured: true,
              }}
              callbacks={{
                onDetail: nav.setDetail,
                onBookmarkChanged: nav.bookmarkChanged,
                onOpenDatabase: nav.searchCurrentPosition,
                onAnalyzeMatch: (match) =>
                  // The Analysis header names the players, like the dialog's fallback.
                  onSetAnalysisSelection({
                    kind: 'single',
                    match: { ...match, display_name: '' },
                  }),
                onTrackJob,
                onNotesSaved: nav.notesSaved,
                onMoveNotes: (target) => {
                  if (navigation.match && navigation.game)
                    nav.setMoveNotes({
                      match: navigation.match,
                      gameNo: target?.gameNo ?? navigation.game.game_no,
                      moveId: target?.moveId,
                      decisionKind: target?.decisionKind,
                      reviewDecision: target?.reviewDecision,
                    });
                },
                onDeckChanged: nav.deckChanged,
                onPosition: nav.openPosition,
                onAnalysisRequestConsumed: (consumed) =>
                  nav.setRequestedAnalysis((pending) =>
                    pending === consumed ? undefined : pending,
                  ),
              }}
              detail={nav.detail}
              notesSuspended={!!nav.moveNotes || view !== 'analysis'}
              scopeRequest={nav.scopeRequest}
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
          visible={view === 'database'}
          uploading={importer.uploading}
          bulkAnalysisProgress={bulkAnalysisProgress}
          revisions={{
            library: nav.revision,
            analysis: assessmentRevision,
            annotation: nav.annotationRevision,
          }}
          context={{
            databaseId: nav.databaseId,
            currentPosition: navigation.position,
            requestedPositionSearch: nav.requestedPositionSearch,
            selectedMatchId: navigation.match?.id,
          }}
          callbacks={{
            onDatabase: nav.setDatabaseId,
            onOpen: nav.openMatch,
            onAnalyzeMatches: onSelectForAnalysis,
            onPosition: nav.openPosition,
            onAnnotation: nav.openAnnotation,
            onImport: importer.chooseFiles,
            onImportFolder: importer.chooseFolder,
            onOpenPlay: (id, resume) => nav.openPlaySession(id, resume),
            onChanged: nav.handleLibraryChanged,
          }}
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
          {nav.opened('players') && (
            <PlayerDirectory
              revision={nav.revision}
              onOpen={nav.openMatch}
              onAnalyzeMatches={onSelectForAnalysis}
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
              aria-pressed={!nav.similarTarget && nav.positionScope === scope}
              onClick={() => {
                nav.setSimilarTarget(undefined);
                nav.setPositionScope(scope);
              }}
            >
              {w(scope)}
            </button>
          ))}
        </div>
        <LazyPanel>
          {nav.decisionSearchOpened && (
            <div hidden={!!nav.similarTarget || nav.positionScope !== 'all'}>
              <DecisionSearch
                visible={
                  view === 'positions' &&
                  !nav.similarTarget &&
                  nav.positionScope === 'all'
                }
                revision={nav.revision}
                bookmarkRevision={nav.bookmarkRevision}
                onBookmarkChanged={nav.bookmarkChanged}
                deckRevision={nav.deckRevision}
                onDeckChanged={nav.deckChanged}
                onStudy={nav.openStudyDeck}
                theme={theme}
                onOpenPosition={(id) => {
                  nav.setDetail('overview');
                  nav.openPosition(id);
                }}
                onOpenCubeDecision={nav.openCubeDecision}
              />
            </div>
          )}
        </LazyPanel>
        <div hidden={!!nav.similarTarget || nav.positionScope === 'all'}>
          <LazyPanel>
            {nav.opened('positions') && (
              <MountHidden
                visible={
                  view === 'positions' &&
                  !nav.similarTarget &&
                  nav.positionScope === 'saved'
                }
              >
                {(visible) => (
                  <PositionBrowser
                    visible={visible}
                    bookmarkRevision={nav.bookmarkRevision}
                    onBookmarkChanged={nav.bookmarkChanged}
                    revision={nav.revision}
                    annotationRevision={nav.annotationRevision}
                    deckRevision={nav.deckRevision}
                    theme={theme}
                    onOpenPosition={(id) => {
                      nav.setDetail('overview');
                      nav.openPosition(id);
                    }}
                    onDeckChanged={nav.deckChanged}
                    onStudy={nav.openStudyDeck}
                    onFindSimilar={nav.searchSimilarPositions}
                  />
                )}
              </MountHidden>
            )}
          </LazyPanel>
        </div>
        <LazyPanel>
          {nav.similarTarget && (
            <SimilarPositions
              key={nav.similarTarget.token}
              source={nav.similarTarget.source}
              matchName={nav.similarTarget.matchName}
              visible={view === 'positions'}
              revision={nav.revision}
              theme={theme}
              onClose={() => nav.setSimilarTarget(undefined)}
              onOpen={(id) => {
                nav.setDetail('overview');
                nav.openPosition(id);
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
          {nav.opened('learning') && (
            <MountHidden visible={view === 'learning'}>
              {(visible) => (
                <StudyWorkspace
                  visible={visible}
                  requestedDeck={nav.requestedDeck}
                  requestedAssessment={nav.requestedAssessment}
                  theme={theme}
                  revision={nav.deckRevision}
                  onChanged={nav.deckChanged}
                  onStudyDeck={nav.openStudyDeck}
                  onOpenPlayers={() => nav.setView('players')}
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
          {nav.opened('extras') && (
            <MountHidden visible={view === 'extras'}>
              {(visible) => (
                <ExtrasWorkspace
                  visible={visible}
                  tool={nav.extrasTool}
                  setTool={nav.setExtrasTool}
                  onOpenMatch={nav.openMatch}
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
      {nav.moveNotes && (
        <LazyPanel>
          <MoveNotesDialog
            target={nav.moveNotes}
            theme={theme}
            onClose={() => nav.setMoveNotes(undefined)}
            onSaved={nav.notesSaved}
          />
        </LazyPanel>
      )}
    </>
  );
}
