import { lazy, useEffect, useId, useState } from 'react';
import { useConfirmedConfiguration } from '../settings/configuration/ConfigurationProvider';
import { useWorkspaceSession } from '../shared/hooks/useWorkspaceSession';
import ProfileSelector from '../settings/profiles/ProfileSelector';
import { openingBookConfigurationKey } from '../settings/configuration/configuration-model';
import { settingsFromConfiguration } from '../settings/configuration/configuration-settings';
import { useBenchmarkActivity } from '../workspaces/extras/useBenchmarkActivity';
import ActionButton from '../shared/ui/ActionButton';
import { TabList, useRovingTabs } from '../shared/ui/TabList';
import { readTheme, saveTheme } from '../settings/ui-preferences';
import type { Theme } from '../shared/board/themes';
import { useJobs } from '../shared/jobs/useJobs';
import { useBackgroundJobs } from './useBackgroundJobs';
import { useI18n } from '../shared/i18n/i18n';
import { useDetailsDismiss } from '../shared/hooks/useDetailsDismiss';
import { useWorkspaceText } from '../shared/i18n/workspace-messages';
import { decodeBulkAnalysisSession } from './workspace-state';
import type { LibraryMatch } from '../workspaces/library/library-api';
import { DEFAULT_ANALYSIS_PLIES } from '../shared/engine/analysis-depth';
import { useUiAppearance } from '../settings/appearance/useUiAppearance';
import { useButtonShortcuts } from './useButtonShortcuts';
import LazyPanel from './LazyPanel';
import type { AnalysisSelection } from './AnalysisDialogs';
import AppFooter from './AppFooter';
import AppNotices from './AppNotices';
import AppWelcome from './AppWelcome';
import MatchImportInputs from './MatchImportInputs';
import { bulkJobPending, useBulkAnalysis } from './useBulkAnalysis';
import { useInstalledSettings } from './useInstalledSettings';
import { useMatchImporter } from './useMatchImporter';
import { useAppNavigation, type AppView } from './useAppNavigation';
import AppWorkspacePanels from './AppWorkspacePanels';
import './workspace.css';

const AnalysisDialogs = lazy(() => import('./AnalysisDialogs'));
const SettingsDialog = lazy(() => import('../settings/SettingsDialog'));

const views: AppView[] = [
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
  const helpNewTabId = useId();
  const confirmed = useConfirmedConfiguration();
  const [settingsCategory, setSettingsCategory] = useState<
    'general' | 'play' | 'profiles'
  >('general');
  const [settingsOpen, setSettingsOpen] = useState(false);
  // The dialog mounts when first opened and then stays mounted.
  const [settingsOpened, setSettingsOpened] = useState(false);
  useEffect(() => {
    if (settingsOpen) setSettingsOpened(true);
  }, [settingsOpen]);
  const benchmarkActivity = useBenchmarkActivity();
  const { t, setLocale } = useI18n();
  const w = useWorkspaceText();
  const { appearance, applyAppearance } = useUiAppearance();
  const [bulkAnalysis, setBulkAnalysis] = useWorkspaceSession(
    'library.bulkAnalysis',
    undefined,
    { jobs: {}, showMatchList: false },
    decodeBulkAnalysisSession,
  );
  const bulkJobIds = Object.keys(bulkAnalysis.jobs);

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
  const bookConfigurationKey = openingBookConfigurationKey(
    confirmed?.opening_book,
  );
  const [analysisSelection, setAnalysisSelection] =
    useState<AnalysisSelection>();

  const tabRoving = useRovingTabs<AppView>({
    items: views,
    active: null,
    onSelect: (next) => nav.selectTab(next),
  });
  const nav = useAppNavigation({
    initialShowMatchList: bulkAnalysis.showMatchList && bulkJobIds.length > 0,
    settingsOpen,
    focusTab: tabRoving.focusTab,
  });
  const headerTabs = useRovingTabs<AppView>({
    items: views,
    active: nav.view === 'home' ? null : nav.view,
    onSelect: nav.selectTab,
  });
  const getHeaderTabRef = (tab: AppView) => (element: HTMLButtonElement | null) => {
    tabRoving.getTabRef(tab)(element);
    headerTabs.getTabRef(tab)(element);
  };

  const importer = useMatchImporter({
    databaseId: nav.databaseId,
    intent: () => nav.navigation.intent() + nav.homeVisits.current,
    onStart: nav.cancelRestoration,
    onOpen: (match) => {
      nav.setView('analysis');
      nav.setDetail('overview');
      void nav.navigation.selectMatch(match);
    },
    onChanged: nav.bumpRevision,
    onBatchFinished: () => nav.setView('database'),
    onError: nav.setError,
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
    databaseId: nav.databaseId,
    tracked: trackedAnalysisJobs,
    missing: missingAnalysisJobs,
    listVisible: nav.view === 'database',
  });
  const analysisInstallationKey = JSON.stringify([
    installedSettings?.version,
    installedSettings?.gnubg_path,
    installedSettings?.analysis_engine,
    settingsRevision,
  ]);
  const jobs = useJobs(nav.navigation.position?.id, analysisEngine, track);
  const footerEngine =
    nav.view === 'play'
      ? (confirmed?.play.engine ?? settings?.play_engine ?? 'gnubg')
      : analysisEngine;

  useEffect(() => {
    if (!confirmed) saveTheme(theme);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [theme, !!confirmed]);
  useEffect(() => {
    const current = confirmed;
    if (!current) return;
    setLocale(current.general.locale);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [confirmed?.general.locale, setLocale]);
  const { bumpBookRevision } = nav;
  useEffect(() => {
    if (bookConfigurationKey !== undefined) bumpBookRevision();
  }, [bookConfigurationKey, bumpBookRevision]);
  useEffect(() => {
    Object.values(jobs.jobs).forEach(track);
  }, [jobs.jobs, track]);

  const welcome = (
    <AppWelcome
      importer={importer}
      restoring={nav.restoring}
      onOpenLibrary={() => {
        nav.cancelRestoration();
        nav.setView('database');
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
        onAccepted={(acceptedJobs) => {
          for (const job of acceptedJobs) track(job);
          nav.bumpRevision();
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
          nav.openMatch(match, undefined, job);
          requestAnimationFrame(() => headerTabs.focusTab('analysis'));
        }}
        onClose={() => setAnalysisSelection(undefined)}
      />
    </LazyPanel>
  );

  const analysisRevision = JSON.stringify([
    assessmentRevision,
    runningAnalysisJobs
      .filter((job) => job.kind === 'match' && job.game_no == null)
      .map((job) => job.id)
      .sort(),
    confirmed?.analysis.error_thresholds,
  ]);

  return (
    <div className="app-shell">
      <a className="skip-to-main" href="#main-content">
        {t('app.skipToMain')}
      </a>
      <header className="app-header">
        <a
          className="brand"
          href="/#home"
          aria-current={nav.view === 'home' ? 'page' : undefined}
          aria-label={t('app.home')}
          onClick={(event) => nav.goHome(event, importer.closeNotice)}
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
          <TabList<AppView>
            as="nav"
            className="app-tabs text-navigation"
            ariaLabel={w('navigation')}
            active={nav.view === 'home' ? null : nav.view}
            onSelect={nav.selectTab}
            roving={{ ...headerTabs, getTabRef: getHeaderTabRef }}
            tabs={views.map((tab) => ({
              value: tab,
              id: `workspace-tab-${tab}`,
              controls: `workspace-panel-${tab}`,
              label: w(tab),
            }))}
          />
          <details
            className="app-help text-navigation"
            ref={helpMenu.ref}
            onKeyDown={helpMenu.onKeyDown}
            onBlur={helpMenu.onBlur}
          >
            <summary>{t('app.help')}</summary>
            <span id={helpNewTabId} className="sr-only">
              {t('app.opensInNewTab')}
            </span>
            <div className="app-help-links">
              <a
                href="https://hendrikreh.github.io/neondojo/user-guide/"
                target="_blank"
                rel="noreferrer"
                aria-describedby={helpNewTabId}
                onClick={helpMenu.close}
              >
                <span>{t('app.neondojoManual')}</span>
                <span className="app-help-external" aria-hidden="true">
                  ↗
                </span>
              </a>
              <a
                href="https://hendrikreh.github.io/neongammon/"
                target="_blank"
                rel="noreferrer"
                aria-describedby={helpNewTabId}
                onClick={helpMenu.close}
              >
                <span>{t('app.neongammonManual')}</span>
                <span className="app-help-external" aria-hidden="true">
                  ↗
                </span>
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
          />
        </div>
        <MatchImportInputs importer={importer} />
      </header>
      {benchmarkActivity.active && (
        <p className="benchmark-global-status" role="status">
          {w('globalBusy')}
        </p>
      )}
      <main
        id="main-content"
        tabIndex={-1}
        className={`app-body app-body-${nav.view}`}
      >
        <AppNotices
          error={nav.error || nav.navigation.error}
          settingsError={settingsError}
          importer={importer}
          onCloseError={() => {
            nav.setError(undefined);
            nav.navigation.clearError();
          }}
          onRetrySettings={reloadSettings}
        />
        <AppWorkspacePanels
          nav={nav}
          theme={theme}
          settings={settings}
          importer={importer}
          jobs={jobs}
          runningAnalysisJobs={runningAnalysisJobs}
          assessmentRevision={assessmentRevision}
          analysisRevision={analysisRevision}
          analysisEngine={analysisEngine}
          analysisPlies={analysisPlies}
          analysisInstallationKey={analysisInstallationKey}
          bulkAnalysisProgress={bulkAnalysisProgress}
          benchmarkActivity={benchmarkActivity}
          welcome={welcome}
          analysisDialogs={analysisDialogs}
          onSelectForAnalysis={selectForAnalysis}
          onSetAnalysisSelection={setAnalysisSelection}
          onTrackJob={track}
          onOpenPlaySettings={() => {
            setSettingsCategory('play');
            setSettingsOpen(true);
          }}
        />
      </main>
      <AppFooter
        key={footerEngine}
        engine={footerEngine}
        settingsOpen={settingsOpen}
        settingsVersion={settings?.version}
      />
      <LazyPanel quiet>
        {(settingsOpen || settingsOpened) && (
          <SettingsDialog
            initialCategory={settingsCategory}
            open={settingsOpen}
            onClose={() => setSettingsOpen(false)}
            onBookRefresh={nav.bumpBookRevision}
            onBenchmark={() => {
              nav.cancelRestoration();
              setSettingsOpen(false);
              nav.setExtrasTool('benchmark');
              nav.setView('extras');
            }}
            theme={theme}
            appearance={appearance}
            onApply={(applied) => {
              setTheme(applied.theme);
              setLocale(applied.locale);
              applyAppearance(applied.appearance);
              applyInstalledSettings(applied.settings);
              if (applied.myProfileChanged) nav.bumpRevision();
            }}
          />
        )}
      </LazyPanel>
    </div>
  );
}
