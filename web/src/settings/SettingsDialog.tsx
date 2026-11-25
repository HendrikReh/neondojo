import LazyPanel from '../app/LazyPanel';
import { useOpenSageInstallation } from './OpenSageInstallation';
import configurationOptions from '../../../shared/configuration-options.json' with { type: 'json' };
import { ConfigurationConflict } from './configuration/configuration-api';
import { useConfigurationErrors } from './profiles/configuration-error-messages';
import { HttpError, isConflict } from '../shared/http/api';
import { AppError } from '../shared/i18n/i18n-core';
import { useOptionalConfiguration } from './configuration/ConfigurationProvider';
import OpeningBookDialog from './OpeningBookDialog';
import EngineSettingsSection, {
  type CheckState,
} from './EngineSettingsSection';
import PlaySettingsSection from './PlaySettingsSection';
import BoardSettingsSection from './BoardSettingsSection';
import ProfileSettingsPanel from './profiles/ProfileSettingsPanel';
import LibraryDialog from '../shared/ui/LibraryDialog';
import { useProfileText } from './profiles/profile-messages';
import {
  applySettingsDraft,
  changedInstallationFields,
  settingsFromConfiguration,
  saveInstallationSettings,
} from './configuration/configuration-settings';
import {
  defaultConfiguration,
  defaultErrorThresholds,
  errorThresholdFields,
  validErrorThresholds,
  type ErrorThresholds,
  type ProfileConfiguration,
} from './configuration/configuration-model';
import { useBenchmarkText } from '../workspaces/extras/benchmark-messages';
import ActionButton from '../shared/ui/ActionButton';
import {
  ANALYSIS_PLIES,
  isAnalysisDepth,
  preferredDepth,
} from '../shared/engine/analysis-depth';
import {
  useEffect,
  useId,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from 'react';
import { useLatest } from '../shared/hooks/useLatest';
import { readPlaySounds, savePlaySounds } from '../shared/board/play-audio';
import { useI18n, languages, type Locale } from '../shared/i18n/i18n';
import { saveLocale, numberFormat } from '../shared/i18n/i18n-core';
import type { Theme } from '../shared/board/themes';
import {
  checkGnubg,
  loadEngines,
  loadSettings,
  saveSettings,
  type AppSettings,
  type EngineInfo,
} from './settings-api';
import {
  depthOptionLabel,
  useSettingsText,
  type SettingsMessageKey,
} from './settings-messages';
import { errorThresholdLabels } from '../shared/review/review-statistics-messages';
import { saveTheme } from './ui-preferences';
import './settings.css';
import {
  openingBookProviderLabel,
  useSourceText,
} from '../workspaces/analysis/candidates/analysis-source-messages';
import {
  uiThemes,
  type UiAppearance,
  type UiMode,
  type UiTheme,
} from './appearance/ui-appearance';
import { useUiAppearanceText } from './appearance/ui-appearance-messages';
import UiAppearancePreview from './appearance/UiAppearancePreview';
import { useVirtualPlayerText } from '../workspaces/players/virtual-player-messages';
import {
  MyPlayerProfileField,
  useMyPlayerProfileDraft,
} from './MyPlayerProfileSetting';

export interface SettingsDialogProps {
  open: boolean;
  initialCategory?: 'general' | 'play' | 'engines' | 'profiles';
  onClose: () => void;
  onBookRefresh: () => void;
  onBenchmark?: () => void;
  theme: Theme;
  appearance: UiAppearance;
  onApply: (value: {
    theme: Theme;
    locale: Locale;
    settings: AppSettings;
    appearance: UiAppearance;
    /** "My player profile" changed, so player-scoped views reload. */
    myProfileChanged: boolean;
  }) => void;
}

type Category =
  | 'general'
  | 'appearance'
  | 'analysis'
  | 'play'
  | 'board'
  | 'engines'
  | 'profiles';
type GuardedDestination = 'close' | 'profiles' | 'book' | 'benchmark';

function profilePreferences(current: ProfileConfiguration) {
  return {
    theme: current.appearance.board_theme,
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

export default function SettingsDialog({
  open,
  onClose,
  theme,
  appearance,
  onApply,
  onBookRefresh,
  onBenchmark,
  initialCategory = 'general',
}: SettingsDialogProps) {
  const configuration = useOptionalConfiguration();
  const configurationRef = useLatest(configuration);
  const profileT = useProfileText();
  const configurationErrors = useConfigurationErrors();
  const { locale, t: appT } = useI18n();
  const [destination, setDestination] = useState<GuardedDestination | null>(
    null,
  );
  const [bookOpen, setBookOpen] = useState(false);
  const bookTrigger = useRef<HTMLButtonElement>(null);
  const dialogTrigger = useRef<HTMLElement | null>(null);
  const categoryButtons = useRef<
    Partial<Record<Category, HTMLButtonElement | null>>
  >({});
  const categories: Category[] = [
    'general',
    'appearance',
    'analysis',
    'play',
    'board',
    'engines',
  ];
  if (configuration) categories.push('profiles');
  const t = useSettingsText();
  const applyLabel = configuration ? profileT('apply') : t('save');
  const sourceT = useSourceText();
  const benchmarkT = useBenchmarkText();
  const appearanceT = useUiAppearanceText();
  const virtualT = useVirtualPlayerText();
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const preferences = useLatest({ theme, locale, appearance });
  const [initialPreferences, setInitialPreferences] = useState({
    theme,
    locale,
    appearance,
  });
  const [category, setCategory] = useState<Category>('general');
  const [draftSounds, setDraftSounds] = useState(readPlaySounds);
  const [initialSounds, setInitialSounds] = useState(readPlaySounds);
  const [draftTheme, setDraftTheme] = useState(theme);
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

  useEffect(() => {
    const element = dialog.current;
    if (!open || !element) return;
    const active =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    const bodyOverflow = document.body.style.overflow;
    setCategory(initialCategory);
    setDestination(null);
    resetPreferences(configurationRef.current?.current);
    element.showModal();
    document.body.style.overflow = 'hidden';
    return () => {
      element.close();
      document.body.style.overflow = bodyOverflow;
      if (active?.isConnected) active.focus();
    };
  }, [open, initialCategory]);

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
  }, [open, reload]);

  const draftPreferences = {
    theme: draftTheme,
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
  useEffect(() => {
    const current = configuration?.current;
    // Profile actions happen inside this category only after the draft guard.
    // Rebase the settings form before returning to its other categories.
    if (!open || category !== 'profiles' || !current) return;
    rebaseFromProfile(current);
  }, [open, configuration?.generation]);
  useEffect(() => {
    if (destination || bookOpen) return;
    if (open) dialogTrigger.current?.focus();
    dialogTrigger.current = null;
  }, [destination, bookOpen, open]);
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
    setDraftTheme(values.theme);
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
    const { settings: draft } = settingsDraft;
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
          draft,
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
          settings = { ...draft, ...savedInstallation };
          setDraft(settings);
          setInitial(
            (previous) => previous && { ...previous, ...savedInstallation },
          );
          await configuration.refresh();
        } else settings = draft;
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
      } else settings = await saveSettings(draft, abort.signal);
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
    const { theme, locale, appearance, sounds } = savedPreferences;
    saveTheme(theme);
    savePlaySounds(sounds);
    saveLocale(locale);
    setDraftTheme(theme);
    setDraftLocale(locale);
    setDraftAppearance(appearance);
    setDraftSounds(sounds);
    setInitialSounds(sounds);
    setInitialPreferences(savedPreferences);
    resetThresholds(savedThresholds, locale);
    setInitial(settings);
    setDraft(settings);
    setSaving(false);
    savingRef.current = false;
    onApply({
      theme,
      locale,
      settings,
      appearance,
      myProfileChanged,
    });
    return true;
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await applyDraft();
  }

  function enterSection(next: Category) {
    const current = configurationRef.current?.current;
    if (category === 'profiles' && next !== 'profiles' && current)
      rebaseFromProfile(current);

    if (next === 'profiles') requestDestination(next);
    else setCategory(next);
  }

  function navigateCategories(
    event: KeyboardEvent<HTMLButtonElement>,
    focused: Category,
  ) {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey)
      return;
    const index = categories.indexOf(focused);
    const last = categories.length - 1;
    let target: number;
    if (event.key === 'ArrowUp') target = Math.max(0, index - 1);
    else if (event.key === 'ArrowDown') target = Math.min(last, index + 1);
    else if (event.key === 'Home') target = 0;
    else if (event.key === 'End') target = last;
    else return;
    event.preventDefault();
    const next = categories[target];
    if (next === focused && next === category) return;
    if (next !== 'profiles' || !dirty) categoryButtons.current[next]?.focus();
    enterSection(next);
  }

  function continueTo(next: GuardedDestination) {
    dialogTrigger.current = null;
    setDestination(null);
    if (next === 'profiles') {
      if (destination === 'profiles')
        dialogTrigger.current = categoryButtons.current.profiles ?? null;
      setCategory(next);
      return;
    }
    if (next === 'book') {
      dialogTrigger.current = bookTrigger.current;
      setBookOpen(true);
      return;
    }
    onClose();
    if (next === 'benchmark') onBenchmark?.();
  }

  function requestDestination(next: GuardedDestination) {
    if (savingRef.current || sage.busy) return;
    if (dirty) {
      dialogTrigger.current =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
      setDestination(next);
    } else continueTo(next);
  }

  function discardDraft(next: GuardedDestination) {
    setDraft(initial);
    setDraftLocale(initialPreferences.locale);
    setDraftTheme(initialPreferences.theme);
    setDraftAppearance(initialPreferences.appearance);
    setDraftSounds(initialSounds);
    setSaveError(null);
    setRecoveryError(null);
    resetInstallationCheck();
    sage.discard();
    myProfile.discard();
    continueTo(next);
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
      setDestination(null);
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

  const saveFeedback = Boolean(saveError) && (
    <div className="settings-error" role="alert">
      <p>
        {t(conflict ? 'conflict' : 'saveError')}{' '}
        {!conflict && diagnostic(saveError)}
      </p>
      {Boolean(recoveryError) && (
        <p>
          {t('loadError')} {diagnostic(recoveryError)}
        </p>
      )}
      {conflict && (
        <ActionButton
          icon="refresh"
          label={t('loadLatest')}
          type="button"
          disabled={saving}
          onClick={() => void reloadLatest()}
        />
      )}
    </div>
  );

  function engineSelect(label: SettingsMessageKey, name: string) {
    const area = name === 'analysis-engine' ? 'analysis' : 'play';
    const selected =
      area === 'analysis' ? draft?.analysis_engine : draft?.play_engine;
    return (
      <label className="settings-field" htmlFor={`${id}-${name}`}>
        <span>{t(label)}</span>
        <select
          aria-label={t(label)}
          id={`${id}-${name}`}
          value={selected}
          onChange={(event) => {
            const value = event.target.value;
            if (!draft || (value !== 'gnubg' && value !== 'bgsage')) return;
            rememberedDepths.current[`${area}-${selected}`] =
              area === 'analysis' ? draft.analysis_plies : draft.play_plies;
            const options = configurationOptions.engine_profiles[value][area];
            const remembered =
              rememberedDepths.current[`${area}-${value}`] ??
              preferredDepth(value);
            const depth = options.includes(remembered)
              ? remembered
              : options[0];
            update(
              area === 'analysis'
                ? { analysis_engine: value, analysis_plies: depth }
                : {
                    play_engine: value,
                    play_plies: depth,
                    play_top_two: false,
                  },
            );
          }}
        >
          {engines.map((item) => (
            <option key={item.id} value={item.id} disabled={!item.available}>
              {item.name}
              {item.available ? '' : ` — ${t('unavailable')}`}
            </option>
          ))}
        </select>
      </label>
    );
  }

  const settingsDialog = (
    <dialog
      ref={dialog}
      className="settings-dialog"
      aria-labelledby={`${id}-title`}
      aria-describedby={`${id}-description`}
      onCancel={(event) => {
        event.preventDefault();
        requestDestination('close');
      }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key !== 'Tab') return;
        const focusable = [
          ...event.currentTarget.querySelectorAll<HTMLElement>(
            'button,input,select,textarea,a[href],summary,[tabindex="0"]',
          ),
        ].filter(
          (element) =>
            !element.matches(':disabled') &&
            element.getClientRects().length > 0,
        );
        const first = focusable[0],
          last = focusable.at(-1);
        if (!first || !last) {
          event.preventDefault();
          return;
        }
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first.focus();
        }
      }}
    >
      <form onSubmit={submit} className="settings-form" noValidate>
        <header className="settings-header">
          <div>
            <h2 id={`${id}-title`}>{t('title')}</h2>
            <p id={`${id}-description`}>{t('description')}</p>
          </div>
          <ActionButton
            icon="close"
            label={t('close')}
            compact
            type="button"
            className="settings-close"
            aria-label={t('close')}
            onClick={() => requestDestination('close')}
            disabled={saving || sage.busy}
            autoFocus
          />
        </header>
        <div className="settings-layout">
          <nav
            className="settings-categories text-navigation text-navigation--stacked"
            aria-label={t('categories')}
          >
            {categories.map((name) => (
              <button
                key={name}
                ref={(button) => {
                  categoryButtons.current[name] = button;
                }}
                type="button"
                aria-current={category === name ? 'page' : undefined}
                className={category === name ? 'active' : ''}
                onClick={() => enterSection(name)}
                onKeyDown={(event) => navigateCategories(event, name)}
                disabled={saving || sage.busy}
              >
                {name === 'profiles'
                  ? profileT('profiles')
                  : name === 'appearance'
                    ? appearanceT('appearance')
                    : t(name)}
              </button>
            ))}
          </nav>
          <div className="settings-content" aria-busy={loading}>
            {category === 'profiles' && <ProfileSettingsPanel />}
            {loading && category !== 'profiles' && (
              <p role="status">{t('loading')}</p>
            )}
            {category !== 'profiles' && Boolean(loadError) && (
              <div className="settings-error" role="alert">
                <p>
                  {t('loadError')} {diagnostic(loadError)}
                </p>
                <ActionButton
                  icon="refresh"
                  label={t('retry')}
                  compact
                  type="button"
                  onClick={() => setReload((value) => value + 1)}
                />
              </div>
            )}
            {draft && (
              <>
                <fieldset
                  className="settings-section"
                  disabled={saving}
                  hidden={category !== 'general'}
                >
                  <legend>{t('general')}</legend>
                  <label className="settings-field" htmlFor={`${id}-language`}>
                    <span>{t('language')}</span>
                    <select
                      id={`${id}-language`}
                      aria-label={t('language')}
                      value={draftLocale}
                      onChange={(event) =>
                        setDraftLocale(event.target.value as Locale)
                      }
                      aria-describedby={`${id}-language-help`}
                    >
                      {languages.map((language) => (
                        <option
                          key={language.id}
                          value={language.id}
                          lang={language.id}
                        >
                          {language.name}
                        </option>
                      ))}
                    </select>
                  </label>
                  <p id={`${id}-language-help`} className="settings-help">
                    {t('languageHelp', { action: applyLabel })}
                  </p>
                  <label className="settings-field" htmlFor={`${id}-player`}>
                    <span>{t('playerName')}</span>
                    <input
                      id={`${id}-player`}
                      value={draft.player_name}
                      maxLength={120}
                      onChange={(event) =>
                        update({ player_name: event.target.value })
                      }
                      aria-describedby={`${id}-player-help`}
                      autoComplete="nickname"
                    />
                  </label>
                  <p id={`${id}-player-help`} className="settings-help">
                    {t('playerHelp')}
                  </p>
                  <MyPlayerProfileField
                    id={`${id}-my-profile`}
                    state={myProfile}
                  />
                </fieldset>
                <fieldset
                  className="settings-section"
                  disabled={saving}
                  hidden={category !== 'appearance'}
                >
                  <legend>{appearanceT('appearance')}</legend>
                  <p className="settings-help settings-intro">
                    {appearanceT('help', { action: applyLabel })}
                  </p>
                  <div className="settings-preview-layout">
                    <div>
                      <label
                        className="settings-field"
                        htmlFor={`${id}-interface-theme`}
                      >
                        <span>{appearanceT('interfaceTheme')}</span>
                        <select
                          id={`${id}-interface-theme`}
                          value={draftAppearance.theme}
                          onChange={(event) =>
                            setDraftAppearance((previous) => ({
                              ...previous,
                              theme: event.target.value as UiTheme,
                            }))
                          }
                        >
                          {uiThemes.map((name) => (
                            <option key={name} value={name}>
                              {appearanceT(name)}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label
                        className="settings-field"
                        htmlFor={`${id}-display-mode`}
                      >
                        <span>{appearanceT('displayMode')}</span>
                        <select
                          id={`${id}-display-mode`}
                          value={draftAppearance.mode}
                          aria-describedby={`${id}-system-help`}
                          onChange={(event) =>
                            setDraftAppearance((previous) => ({
                              ...previous,
                              mode: event.target.value as UiMode,
                            }))
                          }
                        >
                          {(['light', 'dark', 'system'] as const).map(
                            (mode) => (
                              <option key={mode} value={mode}>
                                {appearanceT(mode)}
                              </option>
                            ),
                          )}
                        </select>
                      </label>
                      <p className="settings-help" id={`${id}-system-help`}>
                        {appearanceT('systemHelp')}
                      </p>
                    </div>
                    <UiAppearancePreview appearance={draftAppearance} />
                  </div>
                </fieldset>
                <fieldset
                  className="settings-section"
                  disabled={saving}
                  hidden={category !== 'analysis'}
                >
                  <legend>{t('analysis')}</legend>
                  <div className="settings-field-pair">
                    {engineSelect('analysisEngine', 'analysis-engine')}
                    <label
                      className="settings-field"
                      htmlFor={`${id}-analysis-profile`}
                    >
                      <span>{t('analysisProfile')}</span>
                      <select
                        id={`${id}-analysis-profile`}
                        aria-label={t('analysisProfile')}
                        value={draft.analysis_plies}
                        onChange={(event) =>
                          update({ analysis_plies: Number(event.target.value) })
                        }
                      >
                        {ANALYSIS_PLIES.map((ply) => (
                          <option
                            key={ply}
                            value={ply}
                            disabled={!analysisPlies.includes(ply)}
                          >
                            {depthOptionLabel(t, ply, draft.analysis_engine, [
                              'fast',
                              'deep',
                              'thorough',
                            ])}
                          </option>
                        ))}
                      </select>
                    </label>
                  </div>
                  <p className="settings-help">{t('analysisHelp')}</p>
                  {configuration && (
                    <div
                      className="settings-thresholds"
                      role="group"
                      aria-labelledby={`${id}-thresholds`}
                    >
                      <h3 id={`${id}-thresholds`}>{t('errorThresholds')}</h3>
                      <div className="settings-field-pair">
                        {errorThresholdFields.map((field) => (
                          <label className="settings-field" key={field}>
                            <span>{appT(errorThresholdLabels[field])}</span>
                            <input
                              inputMode="decimal"
                              aria-describedby={`${id}-thresholds-help`}
                              value={thresholdFields[field]}
                              onChange={(event) => {
                                setThresholdFields({
                                  ...thresholdFields,
                                  [field]: event.target.value,
                                });
                                setSaveError(null);
                              }}
                            />
                          </label>
                        ))}
                      </div>
                      <p className="settings-help" id={`${id}-thresholds-help`}>
                        {t('errorThresholdsHelp')}
                      </p>
                    </div>
                  )}
                  <div className="settings-analysis-row">
                    <div>
                      <h3>{sourceT('books')}</h3>
                      {configuration?.current && (
                        <p className="settings-help">
                          {openingBookProviderLabel(
                            locale,
                            configuration.current.opening_book.provider,
                          )}
                        </p>
                      )}
                    </div>
                    <ActionButton
                      ref={bookTrigger}
                      icon="open"
                      label={t('manageBooks')}
                      type="button"
                      onClick={() => requestDestination('book')}
                    />
                  </div>
                  {onBenchmark && (
                    <div className="settings-analysis-row">
                      <h3>{benchmarkT('benchmark')}</h3>
                      <ActionButton
                        icon="open"
                        label={benchmarkT('open')}
                        type="button"
                        onClick={() => requestDestination('benchmark')}
                      />
                    </div>
                  )}
                </fieldset>
                <PlaySettingsSection
                  id={id}
                  draft={draft}
                  update={update}
                  disabled={saving}
                  hidden={category !== 'play'}
                  engineField={engineSelect('playEngine', 'play-engine')}
                  sounds={draftSounds}
                  onSounds={setDraftSounds}
                />
                {open && category === 'board' && (
                  <LazyPanel>
                    <BoardSettingsSection
                      disabled={saving}
                      theme={draftTheme}
                      onTheme={setDraftTheme}
                    />
                  </LazyPanel>
                )}
                <EngineSettingsSection
                  id={id}
                  draft={draft}
                  update={update}
                  disabled={saving}
                  hidden={category !== 'engines'}
                  engines={engines}
                  engineCatalogRefresh={engineCatalogRefresh}
                  onRefreshEngines={() => void refreshEngines()}
                  check={check}
                  checking={checking}
                  gnubgEdited={gnubgEdited}
                  onCheck={checkInstallation}
                  onInstallationEdited={resetInstallationCheck}
                  diagnostic={diagnostic}
                  sage={sage}
                />
              </>
            )}
          </div>
        </div>
        <footer className="settings-footer">
          <div className="settings-feedback">
            {!destination && saveFeedback}
            {dirty && (!saveError || destination) && (
              <p className="settings-help" role="status">
                {t('unsaved')}
              </p>
            )}
            {!!applyHint && (
              <p id={`${id}-apply-target`} className="settings-help">
                {applyHint}
              </p>
            )}
          </div>
          <div className="settings-actions" hidden={category === 'profiles'}>
            <ActionButton
              icon="close"
              label={t('done')}
              type="button"
              onClick={() => requestDestination('close')}
              disabled={saving || sage.busy}
            />
            {(dirty || saving) && (
              <ActionButton
                icon="save"
                label={saving ? t('saving') : applyLabel}
                type="submit"
                className="settings-save"
                aria-describedby={applyHint ? `${id}-apply-target` : undefined}
                disabled={!canApply}
              />
            )}
          </div>
        </footer>
      </form>
    </dialog>
  );
  return (
    <>
      {settingsDialog}
      {open && bookOpen && (
        <OpeningBookDialog
          onClose={() => setBookOpen(false)}
          onRefresh={onBookRefresh}
        />
      )}
      {open && destination && (
        <LibraryDialog
          native
          lockScroll={false}
          title={profileT('draftGuardTitle')}
          onClose={() => setDestination(null)}
          busy={saving}
          initialFocus="dialog"
        >
          <p>{profileT('draftGuardMessage')}</p>
          {!!applyHint && (
            <p id={`${id}-guard-target`} className="settings-help">
              {applyHint}
            </p>
          )}
          {saveFeedback}
          <div className="library-dialog-actions settings-draft-actions">
            <button
              type="button"
              disabled={saving}
              onClick={() => setDestination(null)}
            >
              {t('keepEditing')}
            </button>
            <button
              type="button"
              disabled={saving}
              onClick={() => discardDraft(destination)}
            >
              {profileT('discardDraft')}
            </button>
            <ActionButton
              icon="save"
              label={
                saving
                  ? t('saving')
                  : destination === 'close'
                    ? profileT('apply')
                    : profileT('applyAndContinue')
              }
              type="button"
              className="settings-save"
              aria-describedby={applyHint ? `${id}-guard-target` : undefined}
              disabled={!canApply}
              onClick={async () => {
                if (await applyDraft()) continueTo(destination);
              }}
            />
          </div>
        </LibraryDialog>
      )}
    </>
  );
}
