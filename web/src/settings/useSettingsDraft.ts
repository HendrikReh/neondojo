import { useEffect, useRef, useState } from 'react';
import { ConfigurationConflict } from './configuration/configuration-api';
import { useConfigurationErrors } from './profiles/configuration-error-messages';
import { HttpError, isConflict } from '../shared/http/api';
import { AppError, numberFormat, saveLocale } from '../shared/i18n/i18n-core';
import { useOptionalConfiguration } from './configuration/ConfigurationProvider';
import { useOpenSageInstallation } from './OpenSageInstallation';
import type { CheckState } from './EngineSettingsSection';
import {
  applySettingsDraft,
  changedInstallationFields,
  saveInstallationSettings,
  settingsFromConfiguration,
  type BoardArtworkSelection,
} from './configuration/configuration-settings';
import {
  defaultConfiguration,
  defaultErrorThresholds,
  matchBoardDice,
  validErrorThresholds,
  type ErrorThresholds,
  type ProfileConfiguration,
} from './configuration/configuration-model';
import { isAnalysisDepth } from '../shared/engine/analysis-depth';
import { useLatest } from '../shared/hooks/useLatest';
import {
  readPlaySounds,
  savePlaySounds,
} from '../shared/board/play-sound-preferences';
import { useI18n, type Locale } from '../shared/i18n/i18n';
import type { Theme } from '../shared/board/themes';
import {
  checkGnubg,
  loadEngines,
  loadSettings,
  saveSettings,
  type AppSettings,
  type EngineInfo,
} from './settings-api';
import { useSettingsText } from './settings-messages';
import { saveTheme } from './ui-preferences';
import type { UiAppearance } from './appearance/ui-appearance';
import { useVirtualPlayerText } from '../workspaces/players/virtual-player-messages';
import { useMyPlayerProfileDraft } from './MyPlayerProfileSetting';

export type SettingsApplyPayload = {
  theme: Theme;
  locale: Locale;
  settings: AppSettings;
  appearance: UiAppearance;
  /** "My player profile" changed, so player-scoped views reload. */
  myProfileChanged: boolean;
};

function profilePreferences(current: ProfileConfiguration) {
  return {
    board: {
      theme: current.appearance.board_theme,
      checkerTheme: current.appearance.checker_theme ?? null,
      diceTheme: current.appearance.dice_theme ?? null,
      cubeTheme: current.appearance.cube_theme ?? null,
    },
    locale: current.general.locale,
    appearance: {
      theme: current.appearance.ui_theme,
      mode: current.appearance.mode,
    },
    sounds: current.general.play_sounds,
  };
}

function parseThreshold(value: string): number {
  const decimal = value.trim().replace(',', '.');
  return /^(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)$/.test(decimal)
    ? Number(decimal)
    : NaN;
}

export function useSettingsDraft({
  open,
  inProfilesSection,
  theme,
  appearance,
  onApply,
  onReloadCleared,
}: {
  open: boolean;
  inProfilesSection: boolean;
  theme: Theme;
  appearance: UiAppearance;
  onApply: (value: SettingsApplyPayload) => void;
  onReloadCleared: () => void;
}) {
  const configuration = useOptionalConfiguration();
  const configurationRef = useLatest(configuration);
  const configurationErrors = useConfigurationErrors();
  const { locale } = useI18n();
  const t = useSettingsText();
  const virtualT = useVirtualPlayerText();

  const board: BoardArtworkSelection = {
    theme,
    checkerTheme: null,
    diceTheme: matchBoardDice,
    cubeTheme: null,
  };
  const preferences = useLatest({ board, locale, appearance });
  const [initialPreferences, setInitialPreferences] = useState({
    board,
    locale,
    appearance,
  });
  const [draftSounds, setDraftSounds] = useState(readPlaySounds);
  const [initialSounds, setInitialSounds] = useState(readPlaySounds);
  const [draftBoard, setDraftBoard] = useState(board);
  const [draftLocale, setDraftLocale] = useState(locale);
  const [draftAppearance, setDraftAppearance] = useState(appearance);
  const [initialThresholds, setInitialThresholds] = useState(
    defaultErrorThresholds,
  );
  const [thresholdFields, setThresholdFields] = useState<
    Record<keyof ErrorThresholds, string>
  >({
    questionable: '',
    error: '',
    blunder: '',
    large_blunder: '',
  });
  const errorThresholds: ErrorThresholds = {
    questionable: parseThreshold(thresholdFields.questionable),
    error: parseThreshold(thresholdFields.error),
    blunder: parseThreshold(thresholdFields.blunder),
    large_blunder: parseThreshold(thresholdFields.large_blunder),
  };
  const [draft, setDraft] = useState<AppSettings | null>(null);
  const [initial, setInitial] = useState<AppSettings | null>(null);
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [engineCatalogRefresh, setEngineCatalogRefresh] = useState<{
    error?: unknown;
  } | null>(null);
  const catalogAbort = useRef<AbortController | null>(null);
  const [reload, setReload] = useState(0);
  const sage = useOpenSageInstallation(open, reload, refreshEngines);
  const myProfile = useMyPlayerProfileDraft(open, reload);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<unknown>(null);
  const [recoveryError, setRecoveryError] = useState<unknown>(null);
  const [saveError, setSaveError] = useState<unknown>(null);
  const [saving, setSaving] = useState(false);
  const [checking, setChecking] = useState(false);
  const [check, setCheck] = useState<CheckState>(null);
  const checkAbort = useRef<AbortController | null>(null);
  const saveAbort = useRef<AbortController | null>(null);
  const savingRef = useRef(false);
  const session = useRef(0);
  const rememberedDepths = useRef<Record<string, number>>({});

  function resetThresholds(values: ErrorThresholds, language: Locale) {
    setInitialThresholds(values);
    const formatter = numberFormat(language, {
      minimumFractionDigits: 3,
      maximumFractionDigits: 20,
      useGrouping: false,
    });
    setThresholdFields({
      questionable: formatter.format(values.questionable),
      error: formatter.format(values.error),
      blunder: formatter.format(values.blunder),
      large_blunder: formatter.format(values.large_blunder),
    });
  }

  function resetPreferences(current?: ProfileConfiguration | null) {
    const { sounds, ...values } = current
      ? profilePreferences(current)
      : { ...preferences.current, sounds: readPlaySounds() };
    setInitialPreferences(values);
    setDraftBoard(values.board);
    setDraftLocale(values.locale);
    setDraftAppearance(values.appearance);
    setDraftSounds(sounds);
    setInitialSounds(sounds);
    resetThresholds(
      current?.analysis.error_thresholds ?? defaultErrorThresholds,
      values.locale,
    );
  }

  function rebaseFromProfile(current: ProfileConfiguration) {
    setDraft((previous) =>
      previous ? settingsFromConfiguration(current, previous) : previous,
    );
    setInitial((previous) =>
      previous ? settingsFromConfiguration(current, previous) : previous,
    );
    resetPreferences(current);
  }

  useEffect(() => {
    if (!open) return;
    const currentSession = ++session.current;
    const abort = new AbortController();
    setLoading(true);
    setDraft(null);
    setInitial(null);
    setEngines([]);
    setEngineCatalogRefresh(null);
    setLoadError(null);
    setRecoveryError(null);
    setSaveError(null);
    setCheck(null);
    setChecking(false);
    setSaving(false);
    savingRef.current = false;
    Promise.all([loadSettings(abort.signal), loadEngines(abort.signal)])
      .then(([settings, catalog]) => {
        if (abort.signal.aborted || session.current !== currentSession) return;
        const latest = configurationRef.current?.current;
        const effective = latest
          ? settingsFromConfiguration(latest, settings)
          : settings;
        resetPreferences(latest);
        setDraft(effective);
        setInitial(effective);
        setEngines(catalog);
      })
      .catch((error: unknown) => {
        if (!abort.signal.aborted && session.current === currentSession)
          setLoadError(error);
      })
      .finally(() => {
        if (!abort.signal.aborted && session.current === currentSession)
          setLoading(false);
      });
    return () => {
      ++session.current;
      abort.abort();
      checkAbort.current?.abort();
      saveAbort.current?.abort();
      catalogAbort.current?.abort();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, reload]);

  useEffect(() => {
    const current = configuration?.current;
    // Profile actions happen inside this category only after the draft guard.
    // Rebase the settings form before returning to its other categories.
    if (!open || !inProfilesSection || !current) return;
    rebaseFromProfile(current);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, configuration?.generation]);

  const draftPreferences = {
    board: draftBoard,
    locale: draftLocale,
    appearance: draftAppearance,
    sounds: draftSounds,
  };
  const initialDraft = initial && {
    ...initialPreferences,
    sounds: initialSounds,
    settings: initial,
    errorThresholds: initialThresholds,
  };
  const settingsDraft = draft && {
    ...draftPreferences,
    settings: draft,
    errorThresholds,
  };
  const baseline =
    initialDraft && applySettingsDraft(defaultConfiguration, initialDraft);
  const configurationChanged = !!(
    baseline &&
    settingsDraft &&
    initialDraft &&
    JSON.stringify(
      applySettingsDraft(baseline, settingsDraft, initialDraft),
    ) !== JSON.stringify(baseline)
  );
  const installationChanged =
    sage.dirty ||
    !!(draft && initial && changedInstallationFields(initial, draft).length);
  const dirty =
    configurationChanged || installationChanged || myProfile.changed;
  const applyHint = [
    configurationChanged && t('applyConfiguration'),
    installationChanged && t('applyInstallation'),
    myProfile.changed && virtualT('applyMyProfile'),
  ]
    .filter(Boolean)
    .join(' ');
  const conflict = isConflict(saveError);
  // Only the configuration endpoints send a code; an engine settings conflict
  // needs the latest values first.
  const canRetryConflict =
    configurationChanged &&
    (saveError instanceof ConfigurationConflict ||
      (saveError instanceof HttpError && !!saveError.code));
  const canApply =
    !!draft &&
    dirty &&
    !loading &&
    !checking &&
    !saving &&
    !sage.busy &&
    (!conflict || canRetryConflict) &&
    (!configuration || configuration.ready);
  const gnubgEdited = !!(
    draft &&
    initial &&
    changedInstallationFields(initial, draft).some((field) =>
      field.startsWith('gnubg_'),
    )
  );
  const analysisPlies =
    engines
      .find((e) => e.id === draft?.analysis_engine)
      ?.plies.filter(isAnalysisDepth) ?? [];

  function diagnostic(error: unknown) {
    if (error instanceof AppError && error.key === 'error.invalidResponse')
      return t('invalidResponse');
    // Engine and installation errors have no code and say what is wrong.
    if (error instanceof HttpError && !error.code) return error.diagnostic;
    if (!(error instanceof AppError))
      return error instanceof Error ? error.message : '';
    return [
      configurationErrors.message(error),
      ...configurationErrors.issues(error).map(configurationErrors.issue),
    ].join(' ');
  }

  function update(values: Partial<AppSettings>) {
    setDraft((previous) => (previous ? { ...previous, ...values } : previous));
    if (!conflict) setSaveError(null);
  }

  function resetInstallationCheck() {
    checkAbort.current?.abort();
    setChecking(false);
    setCheck(null);
  }

  async function refreshEngines() {
    catalogAbort.current?.abort();
    const abort = new AbortController();
    catalogAbort.current = abort;
    setEngineCatalogRefresh({});
    try {
      const value = await loadEngines(abort.signal);
      if (abort.signal.aborted) return;
      setEngines(value);
      setEngineCatalogRefresh(null);
    } catch (error) {
      if (!abort.signal.aborted) setEngineCatalogRefresh({ error });
    }
  }

  async function checkInstallation() {
    if (!draft || checking || savingRef.current) return;
    checkAbort.current?.abort();
    const abort = new AbortController();
    checkAbort.current = abort;
    const currentSession = session.current;
    const path = draft.gnubg_path;
    setChecking(true);
    setCheck(null);
    try {
      // Playing and analysis may use different thread counts; check each one.
      const threadCounts = [
        ...new Set([
          draft.gnubg_threads ?? null,
          draft.gnubg_play_threads ?? null,
        ]),
      ];
      let result = { version: '' };
      for (const threads of threadCounts)
        result = await checkGnubg(path, abort.signal, threads);
      if (!abort.signal.aborted && currentSession === session.current) {
        setCheck({ path, version: result.version });
      }
    } catch (error) {
      if (!abort.signal.aborted && currentSession === session.current)
        setCheck({ path, error });
    } finally {
      if (!abort.signal.aborted && currentSession === session.current)
        setChecking(false);
    }
  }

  async function applyDraft(): Promise<boolean> {
    if (!settingsDraft || !initialDraft || !canApply || savingRef.current)
      return false;
    const { settings: currentDraft } = settingsDraft;
    if (!validErrorThresholds(errorThresholds)) {
      setSaveError(new Error(t('invalidErrorThresholds')));
      return false;
    }
    savingRef.current = true;
    setSaving(true);
    setSaveError(null);
    const abort = new AbortController();
    saveAbort.current = abort;
    const currentSession = session.current;
    let settings: AppSettings;
    let savedPreferences = draftPreferences;
    let savedThresholds = initialThresholds;
    const myProfileChanged = myProfile.changed;
    try {
      if (myProfileChanged) await myProfile.save(abort.signal);
      if (sage.dirty) await sage.verify();
      if (configuration) {
        // Machine installation is separate from the profile snapshot.
        const installation = await saveInstallationSettings(
          initialDraft.settings,
          currentDraft,
          abort.signal,
        );
        if (installation) {
          const savedInstallation = {
            version: installation.version,
            gnubg_path: installation.gnubg_path,
            gnubg_threads: installation.gnubg_threads,
            gnubg_play_threads: installation.gnubg_play_threads,
            open_sage_threads: installation.open_sage_threads,
            open_sage_play_threads: installation.open_sage_play_threads,
            parallel_analysis_jobs: installation.parallel_analysis_jobs,
          };
          settings = { ...currentDraft, ...savedInstallation };
          setDraft(settings);
          setInitial(
            (previous) => previous && { ...previous, ...savedInstallation },
          );
          await configuration.refresh();
        } else settings = currentDraft;
        if (configurationChanged) {
          const saved = await configuration.applyCurrent(
            (current) =>
              applySettingsDraft(current, settingsDraft, initialDraft),
            { rebaseOnConflict: true },
          );
          settings = settingsFromConfiguration(saved, settings);
          savedPreferences = profilePreferences(saved);
          savedThresholds = saved.analysis.error_thresholds;
        } else {
          const current = configurationRef.current?.current;
          if (current) {
            settings = settingsFromConfiguration(current, settings);
            savedPreferences = profilePreferences(current);
            savedThresholds = current.analysis.error_thresholds;
          }
        }
      } else settings = await saveSettings(currentDraft, abort.signal);
    } catch (error) {
      if (!abort.signal.aborted && session.current === currentSession) {
        setSaveError(error);
        setSaving(false);
        savingRef.current = false;
      }
      return false;
    }
    if (abort.signal.aborted || session.current !== currentSession)
      return false;
    const {
      board: savedBoard,
      locale: savedLocale,
      appearance: savedAppearance,
      sounds: savedSounds,
    } = savedPreferences;
    const { theme: savedTheme } = savedBoard;
    saveTheme(savedTheme);
    savePlaySounds(savedSounds);
    saveLocale(savedLocale);
    setDraftBoard(savedBoard);
    setDraftLocale(savedLocale);
    setDraftAppearance(savedAppearance);
    setDraftSounds(savedSounds);
    setInitialSounds(savedSounds);
    setInitialPreferences(savedPreferences);
    resetThresholds(savedThresholds, savedLocale);
    setInitial(settings);
    setDraft(settings);
    setSaving(false);
    savingRef.current = false;
    onApply({
      theme: savedTheme,
      locale: savedLocale,
      settings,
      appearance: savedAppearance,
      myProfileChanged,
    });
    return true;
  }

  function discardDraft() {
    setDraft(initial);
    setDraftLocale(initialPreferences.locale);
    setDraftBoard(initialPreferences.board);
    setDraftAppearance(initialPreferences.appearance);
    setDraftSounds(initialSounds);
    resetThresholds(initialThresholds, initialPreferences.locale);
    setSaveError(null);
    setRecoveryError(null);
    resetInstallationCheck();
    sage.discard();
    myProfile.discard();
  }

  async function reloadLatest() {
    if (savingRef.current) return;
    const currentSession = session.current;
    savingRef.current = true;
    setSaving(true);
    setRecoveryError(null);
    try {
      await configuration?.reloadServer();
      if (session.current !== currentSession) return;
      onReloadCleared();
      setReload((value) => value + 1);
    } catch (error) {
      if (session.current === currentSession) setRecoveryError(error);
    } finally {
      if (session.current === currentSession) {
        savingRef.current = false;
        setSaving(false);
      }
    }
  }

  return {
    configuration,
    configurationRef,
    draft,
    engines,
    engineCatalogRefresh,
    sage,
    myProfile,
    loading,
    loadError,
    recoveryError,
    saveError,
    setSaveError,
    saving,
    savingRef,
    checking,
    check,
    dirty,
    applyHint,
    conflict,
    canApply,
    gnubgEdited,
    analysisPlies,
    draftBoard,
    setDraftBoard,
    draftLocale,
    setDraftLocale,
    draftAppearance,
    setDraftAppearance,
    draftSounds,
    setDraftSounds,
    thresholdFields,
    setThresholdFields,
    rememberedDepths,
    resetPreferences,
    rebaseFromProfile,
    diagnostic,
    update,
    resetInstallationCheck,
    refreshEngines,
    checkInstallation,
    applyDraft,
    discardDraft,
    reloadLatest,
    triggerReload: () => setReload((value) => value + 1),
  };
}
