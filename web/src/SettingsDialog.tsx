import OpenSageInstallation, {
  useOpenSageInstallation,
} from './OpenSageInstallation';
import configurationOptions from '../../shared/configuration-options.json' with { type: 'json' };
import { ConfigurationApiError } from './configuration-api';
import { useOptionalConfiguration } from './ConfigurationProvider';
import OpeningBookDialog from './OpeningBookDialog';
import ProfileSettingsPanel from './ProfileSettingsPanel';
import EngineLabel from './EngineLabel';
import LibraryDialog from './LibraryDialog';
import { useProfileText } from './profile-messages';
import {
  applySettingsDraft,
  changedInstallationFields,
  settingsFromConfiguration,
  saveInstallationSettings,
} from './configuration-settings';
import {
  defaultConfiguration,
  type ProfileConfiguration,
} from './configuration-model';
import { useBenchmarkText } from './benchmark-messages';
import ActionButton from './ActionButton';
import { ANALYSIS_PLIES, isAnalysisDepth } from './analysis-depth';
import { useEffect, useId, useRef, useState, type FormEvent } from 'react';
import Board from './Board';
import { readPlaySounds, savePlaySounds } from './play-audio';
import { usePlayText } from './play-messages';
import {
  playModes,
  usePlayModeText,
  type PlayMode,
} from './play-mode-messages';
import type { Position } from './api';
import { useI18n, languages, type Locale } from './i18n';
import { saveLocale } from './i18n-core';
import { archiveThemes, themes, type Theme } from './themes';
import {
  checkGnubg,
  loadEngines,
  loadSettings,
  saveSettings,
  SettingsApiError,
  type AppSettings,
  type EngineInfo,
} from './settings-api';
import { useSettingsText, type SettingsMessageKey } from './settings-messages';
import { saveTheme } from './ui-preferences';
import './settings.css';
import {
  openingBookProviderLabel,
  useSourceText,
} from './analysis-source-messages';
import {
  uiThemes,
  type UiAppearance,
  type UiMode,
  type UiTheme,
} from './ui-appearance';
import { useUiAppearanceText } from './ui-appearance-messages';
import UiAppearancePreview from './UiAppearancePreview';

export interface SettingsDialogProps {
  open: boolean;
  initialCategory?: 'general' | 'profiles';
  onClose: () => void;
  onBookSaved: () => void;
  onBenchmark?: () => void;
  theme: Theme;
  appearance: UiAppearance;
  onApply: (value: {
    theme: Theme;
    locale: Locale;
    settings: AppSettings;
    appearance: UiAppearance;
  }) => void;
}

// A fixed, legal GNUbg position with checkers on the bar and borne off on both
// sides. It belongs only to the preview; it never enters the match library.
const previewPosition: Position = {
  id: '00000000-0000-0000-0000-000000000001',
  match_uuid: '00000000-0000-0000-0000-000000000002',
  board: [
    -1, -1, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -4, 4, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0,
    1, 1,
  ],
  dice: [3, 2],
  player: 0,
  game_no: 1,
  ordinal: 0,
  ply: 0,
  position_id: '4HPwACjwOXgAFA',
  gnubg_match_id: 'EYHpACAAGAAE',
  notation: '',
};

type Category =
  'general' | 'appearance' | 'analysis' | 'play' | 'board' | 'profiles';
type GuardedDestination = 'close' | 'profiles' | 'book' | 'benchmark';
type CheckState =
  { path: string; version: string } | { path: string; error: unknown } | null;

export default function SettingsDialog({
  open,
  onClose,
  theme,
  appearance,
  onApply,
  onBookSaved,
  onBenchmark,
  initialCategory = 'general',
}: SettingsDialogProps) {
  const configuration = useOptionalConfiguration();
  const configurationRef = useRef(configuration);
  configurationRef.current = configuration;
  const profileT = useProfileText();
  const { locale } = useI18n();
  const [destination, setDestination] = useState<GuardedDestination | null>(
    null,
  );
  const [bookOpen, setBookOpen] = useState(false);
  const bookTrigger = useRef<HTMLButtonElement>(null);
  const dialogTrigger = useRef<HTMLElement | null>(null);
  const t = useSettingsText();
  const playT = usePlayText();
  const modeT = usePlayModeText();
  const sourceT = useSourceText();
  const benchmarkT = useBenchmarkText();
  const appearanceT = useUiAppearanceText();
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const preferences = useRef({ theme, locale, appearance });
  preferences.current = { theme, locale, appearance };
  const initialPreferences = useRef(preferences.current);
  const [category, setCategory] = useState<Category>('general');
  const [draftSounds, setDraftSounds] = useState(readPlaySounds);
  const [initialSounds, setInitialSounds] = useState(readPlaySounds);
  const [draftTheme, setDraftTheme] = useState(theme);
  const [draftLocale, setDraftLocale] = useState(locale);
  const [draftAppearance, setDraftAppearance] = useState(appearance);
  const [draft, setDraft] = useState<AppSettings | null>(null);
  const [initial, setInitial] = useState<AppSettings | null>(null);
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [engineCatalogError, setEngineCatalogError] = useState<unknown>(null);
  const [reload, setReload] = useState(0);
  const sage = useOpenSageInstallation(open, reload);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<unknown>(null);
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
    setEngineCatalogError(null);
    setLoadError(null);
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
    };
  }, [open, reload]);

  const draftPreferences = {
    theme: draftTheme,
    locale: draftLocale,
    appearance: draftAppearance,
    sounds: draftSounds,
  };
  const configurationChanged = Boolean(
    draft &&
    initial &&
    JSON.stringify(
      applySettingsDraft(defaultConfiguration, {
        ...draftPreferences,
        settings: draft,
      }),
    ) !==
      JSON.stringify(
        applySettingsDraft(defaultConfiguration, {
          ...initialPreferences.current,
          sounds: initialSounds,
          settings: initial,
        }),
      ),
  );
  const installationChanged =
    sage.dirty ||
    !!(draft && initial && changedInstallationFields(initial, draft).length);
  const dirty = configurationChanged || installationChanged;
  const applyHint = [
    configurationChanged && t('applyConfiguration'),
    installationChanged && t('applyInstallation'),
  ]
    .filter(Boolean)
    .join(' ');
  const conflict =
    (saveError instanceof SettingsApiError ||
      saveError instanceof ConfigurationApiError) &&
    saveError.status === 409;
  const canApply =
    !!draft &&
    dirty &&
    !loading &&
    !checking &&
    !saving &&
    !sage.busy &&
    !conflict &&
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
  const engine = engines.find(({ id }) => id === 'gnubg');
  const selectedTheme = themes.find(({ id }) => id === draftTheme) ?? themes[0];
  const analysisPlies =
    engines
      .find((e) => e.id === draft?.analysis_engine)
      ?.plies.filter(isAnalysisDepth) ?? [];
  const playPlies = draft
    ? configurationOptions.engine_profiles[draft.play_engine].play
    : [];
  const rememberedDepths = useRef<Record<string, number>>({});

  function resetPreferences(current?: ProfileConfiguration | null) {
    const values = current
      ? {
          theme: current.appearance.board_theme,
          locale: current.general.locale,
          appearance: {
            theme: current.appearance.ui_theme,
            mode: current.appearance.mode,
          },
        }
      : preferences.current;
    const sounds = current?.general.play_sounds ?? readPlaySounds();
    initialPreferences.current = values;
    setDraftTheme(values.theme);
    setDraftLocale(values.locale);
    setDraftAppearance(values.appearance);
    setDraftSounds(sounds);
    setInitialSounds(sounds);
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
    if (error instanceof SettingsApiError && error.status === 200)
      return t('invalidResponse');
    return error instanceof Error ? error.message : '';
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
    const currentSession = session.current;
    try {
      const value = await loadEngines();
      if (currentSession !== session.current) return;
      setEngines(value);
      setEngineCatalogError(null);
    } catch (error) {
      if (currentSession === session.current) setEngineCatalogError(error);
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
      const result = await checkGnubg(path, abort.signal, draft.gnubg_threads);
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
    if (!draft || !canApply || savingRef.current) return false;
    if (
      draft.tutor_threshold !== undefined &&
      (!Number.isFinite(draft.tutor_threshold) ||
        draft.tutor_threshold <= 0 ||
        draft.tutor_threshold > 1)
    ) {
      setSaveError(new Error(modeT('thresholdHelp')));
      return false;
    }
    savingRef.current = true;
    setSaving(true);
    setSaveError(null);
    const abort = new AbortController();
    saveAbort.current = abort;
    const currentSession = session.current;
    let settings: AppSettings;
    try {
      if (sage.dirty) {
        await sage.verify();
        void refreshEngines();
      }
      if (configuration) {
        // Machine installation is separate from the profile snapshot.
        const installation =
          initial &&
          (await saveInstallationSettings(initial, draft, abort.signal));
        if (installation) {
          const savedInstallation = {
            version: installation.version,
            gnubg_path: installation.gnubg_path,
            gnubg_threads: installation.gnubg_threads,
            open_sage_threads: installation.open_sage_threads,
          };
          settings = { ...draft, ...savedInstallation };
          setDraft(settings);
          setInitial(
            (previous) => previous && { ...previous, ...savedInstallation },
          );
          await configuration.refresh();
        } else settings = draft;
        if (configurationChanged) {
          await configuration.applyCurrent((current) =>
            applySettingsDraft(current, {
              ...draftPreferences,
              settings: draft,
            }),
          );
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
    saveTheme(draftTheme);
    savePlaySounds(draftSounds);
    setInitialSounds(draftSounds);
    saveLocale(draftLocale);
    initialPreferences.current = {
      theme: draftTheme,
      locale: draftLocale,
      appearance: draftAppearance,
    };
    setInitial(settings);
    setDraft(settings);
    setSaving(false);
    savingRef.current = false;
    onApply({
      theme: draftTheme,
      locale: draftLocale,
      settings,
      appearance: draftAppearance,
    });
    return true;
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (await applyDraft()) onClose();
  }

  function enterSection(next: Category) {
    const current = configurationRef.current?.current;
    if (category === 'profiles' && next !== 'profiles' && current)
      rebaseFromProfile(current);

    if (next === 'profiles') requestDestination(next);
    else setCategory(next);
  }

  function continueTo(next: GuardedDestination) {
    dialogTrigger.current = null;
    setDestination(null);
    if (next === 'profiles') {
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
    setDraftLocale(initialPreferences.current.locale);
    setDraftTheme(initialPreferences.current.theme);
    setDraftAppearance(initialPreferences.current.appearance);
    setDraftSounds(initialSounds);
    setSaveError(null);
    resetInstallationCheck();
    sage.discard();
    continueTo(next);
  }

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
              rememberedDepths.current[`${area}-${value}`] ?? 2;
            const depth = options.includes(remembered)
              ? remembered
              : options[0];
            update(
              area === 'analysis'
                ? { analysis_engine: value, analysis_plies: depth }
                : { play_engine: value, play_plies: depth },
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
            {(
              [
                'general',
                'appearance',
                'analysis',
                'play',
                'board',
                ...(configuration ? ['profiles' as const] : []),
              ] as const
            ).map((name) => (
              <button
                key={name}
                type="button"
                aria-current={category === name ? 'page' : undefined}
                className={category === name ? 'active' : ''}
                onClick={() => enterSection(name)}
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
                    {t('languageHelp')}
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
                </fieldset>
                <fieldset
                  className="settings-section"
                  disabled={saving}
                  hidden={category !== 'appearance'}
                >
                  <legend>{appearanceT('appearance')}</legend>
                  <p className="settings-help settings-intro">
                    {appearanceT('help')}
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
                            {draft.analysis_engine === 'bgsage' ||
                            ply === 3 ||
                            ply === 4
                              ? `${ply}-ply`
                              : t((['fast', 'deep', 'thorough'] as const)[ply])}
                          </option>
                        ))}
                      </select>
                    </label>
                  </div>
                  <p className="settings-help">{t('analysisHelp')}</p>
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
                  <details className="settings-engine-setup">
                    <summary>
                      <span>{t('localEngines')}</span>
                      <span className="settings-engine-summary">
                        {(
                          [
                            ['gnubg', t('installation')],
                            ['bgsage', t('planned')],
                          ] as const
                        ).map(([provider, label]) => {
                          const available = engines.find(
                            ({ id }) => id === provider,
                          )?.available;
                          return (
                            <span
                              key={provider}
                              className={
                                available ? 'settings-success' : undefined
                              }
                            >
                              {label} ·{' '}
                              {t(available ? 'available' : 'unavailable')}
                            </span>
                          );
                        })}
                      </span>
                    </summary>
                    <div className="settings-engine-details">
                      <fieldset className="settings-section settings-engine">
                        <legend>{t('installation')}</legend>
                        <div className="settings-engine-path">
                          <label
                            className="settings-field"
                            htmlFor={`${id}-path`}
                          >
                            <span>{t('path')}</span>
                            <input
                              id={`${id}-path`}
                              required
                              value={draft.gnubg_path}
                              spellCheck={false}
                              autoComplete="off"
                              aria-describedby={`${id}-path-help ${id}-check-status`}
                              aria-invalid={Boolean(check && 'error' in check)}
                              onChange={(event) => {
                                resetInstallationCheck();
                                update({ gnubg_path: event.target.value });
                              }}
                            />
                          </label>
                          <ActionButton
                            icon="confirm"
                            label={t('check')}
                            type="button"
                            disabled={checking || !draft.gnubg_path.trim()}
                            onClick={checkInstallation}
                          />
                        </div>
                        <p id={`${id}-path-help`} className="settings-help">
                          {t('pathHelp')}
                        </p>
                        <div
                          className="settings-engine-status"
                          id={`${id}-check-status`}
                        >
                          {engine?.version &&
                            !check &&
                            draft.gnubg_path === initial?.gnubg_path &&
                            (draft.gnubg_threads ?? null) ===
                              (initial?.gnubg_threads ?? null) && (
                              <p>
                                <EngineLabel
                                  details={engine.version}
                                  format={(version) =>
                                    t('engineVersion', { version })
                                  }
                                />
                              </p>
                            )}
                          {!engine?.available && <p>{t('unavailable')}</p>}
                          {check && 'version' in check && (
                            <p role="status" className="settings-success">
                              <EngineLabel
                                details={check.version}
                                format={(version) =>
                                  t('checkSuccess', { version })
                                }
                              />
                            </p>
                          )}
                          {check && 'error' in check && (
                            <p role="alert" className="settings-error">
                              {t('checkError')} {diagnostic(check.error)}
                            </p>
                          )}
                          {!check &&
                            (draft.gnubg_path !== initial?.gnubg_path ||
                              (draft.gnubg_threads ?? null) !==
                                (initial?.gnubg_threads ?? null)) && (
                              <p>{t('checkNeeded')}</p>
                            )}
                          {checking && <p role="status">{t('checking')}</p>}
                        </div>
                        <p className="settings-help">{t('checkHelp')}</p>
                      </fieldset>
                      <OpenSageInstallation
                        setup={sage}
                        onVerified={refreshEngines}
                      />
                      <p className="settings-help">{t('localEnginesHelp')}</p>
                      <div className="settings-field-pair">
                        {(
                          [
                            ['gnubg', 'gnubg_threads', 'gnubgThreads'],
                            ['bgsage', 'open_sage_threads', 'sageThreads'],
                          ] as const
                        ).map(([provider, field, label]) => {
                          const { min } =
                            configurationOptions.engine_threads[provider];
                          const max = engines.find(
                            (engine) => engine.id === provider,
                          )?.max_parallel_threads;
                          const count = draft[field];
                          const validRange = max != null && max >= min;
                          const retained =
                            count != null &&
                            (!validRange || count < min || count > max);
                          const labelId = `${id}-${field}-label`;
                          const limitsId = `${id}-${field}-limits`;
                          return (
                            <label className="settings-field" key={field}>
                              <span className="settings-thread-label">
                                <span id={labelId}>{t(label)}</span>
                                <span className="settings-help" id={limitsId}>
                                  {validRange
                                    ? t('configurableThreadRange', { min, max })
                                    : t('threadRangeUnavailable')}
                                  {retained && (
                                    <> {t('retainedThreadCount', { count })}</>
                                  )}
                                </span>
                              </span>
                              <select
                                value={count ?? ''}
                                disabled={!validRange && count == null}
                                aria-labelledby={labelId}
                                aria-describedby={`${limitsId} ${id}-threads-help`}
                                onChange={(event) => {
                                  if (provider === 'gnubg')
                                    resetInstallationCheck();
                                  const value = event.target.value;
                                  update({
                                    [field]:
                                      value === '' ? null : Number(value),
                                  });
                                }}
                              >
                                <option value="">{t('defaultThreads')}</option>
                                {retained && (
                                  <option value={count} disabled>
                                    {count}
                                  </option>
                                )}
                                {validRange &&
                                  Array.from(
                                    { length: max - min + 1 },
                                    (_, index) => min + index,
                                  ).map((threads) => (
                                    <option key={threads} value={threads}>
                                      {threads}
                                    </option>
                                  ))}
                              </select>
                            </label>
                          );
                        })}
                      </div>
                      <p className="settings-help" id={`${id}-threads-help`}>
                        {t('threadsHelp')}
                      </p>
                    </div>
                  </details>
                  {Boolean(engineCatalogError) && (
                    <div className="settings-error" role="alert">
                      <p>
                        {t('engineStatusError')}{' '}
                        {diagnostic(engineCatalogError)}
                      </p>
                      <button
                        type="button"
                        onClick={() => void refreshEngines()}
                      >
                        {t('retry')}
                      </button>
                    </div>
                  )}
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
                <fieldset
                  className="settings-section"
                  disabled={saving}
                  hidden={category !== 'play'}
                >
                  <legend>{t('play')}</legend>
                  <div className="settings-field-pair">
                    {engineSelect('playEngine', 'play-engine')}
                    <label
                      className="settings-field"
                      htmlFor={`${id}-play-strength`}
                    >
                      <span>{t('playStrength')}</span>
                      <select
                        id={`${id}-play-strength`}
                        aria-label={t('playStrength')}
                        value={draft.play_plies}
                        onChange={(event) =>
                          update({ play_plies: Number(event.target.value) })
                        }
                      >
                        {[0, 1, 2].map((ply) => (
                          <option
                            key={ply}
                            value={ply}
                            disabled={!playPlies.includes(ply)}
                          >
                            {draft.play_engine === 'bgsage'
                              ? `${ply}-ply`
                              : t(
                                  (
                                    [
                                      'strength0',
                                      'strength1',
                                      'strength2',
                                    ] as const
                                  )[ply],
                                )}
                          </option>
                        ))}
                      </select>
                    </label>
                  </div>
                  <p className="settings-help">{playT('playHelp')}</p>
                  <section
                    className="settings-play-group"
                    role="group"
                    aria-labelledby={`${id}-play-feedback`}
                  >
                    <h3 id={`${id}-play-feedback`}>{t('playFeedbackGroup')}</h3>
                    <div className="settings-field-pair">
                      <div>
                        <label className="settings-field">
                          <span>{modeT('defaultMode')}</span>
                          <select
                            aria-label={modeT('defaultMode')}
                            aria-describedby={`${id}-play-mode-help`}
                            value={draft.default_play_mode ?? 'coach'}
                            onChange={(event) =>
                              update({
                                default_play_mode: event.target
                                  .value as PlayMode,
                              })
                            }
                          >
                            {playModes.map((mode) => (
                              <option key={mode} value={mode}>
                                {modeT(mode)}
                              </option>
                            ))}
                          </select>
                        </label>
                        <p
                          className="settings-help"
                          id={`${id}-play-mode-help`}
                        >
                          {modeT(`${draft.default_play_mode ?? 'coach'}Help`)}
                        </p>
                      </div>
                      <div>
                        <label className="settings-field">
                          <span>{modeT('threshold')}</span>
                          <input
                            type="number"
                            aria-describedby={`${id}-play-threshold-help`}
                            min="0.001"
                            max="1"
                            step="0.001"
                            value={draft.tutor_threshold ?? 0.04}
                            onChange={(event) =>
                              update({
                                tutor_threshold: event.target.valueAsNumber,
                              })
                            }
                          />
                        </label>
                        <p
                          className="settings-help"
                          id={`${id}-play-threshold-help`}
                        >
                          {modeT('thresholdHelp')}
                        </p>
                      </div>
                      <div className="settings-play-wide">
                        <label className="settings-play-toggle">
                          <input
                            type="checkbox"
                            aria-describedby={`${id}-play-automatic-help`}
                            checked={draft.training_auto_analysis ?? true}
                            onChange={(event) =>
                              update({
                                training_auto_analysis: event.target.checked,
                              })
                            }
                          />
                          <span>{modeT('automatic')}</span>
                        </label>
                        <p
                          className="settings-help"
                          id={`${id}-play-automatic-help`}
                        >
                          {modeT('automaticHelp')}
                        </p>
                      </div>
                    </div>
                  </section>
                  <section
                    className="settings-play-group"
                    role="group"
                    aria-labelledby={`${id}-play-preferences`}
                  >
                    <h3 id={`${id}-play-preferences`}>
                      {t('playPreferencesGroup')}
                    </h3>
                    <div className="settings-field-pair">
                      <div>
                        <label className="settings-play-toggle">
                          <input
                            type="checkbox"
                            aria-describedby={`${id}-play-sounds-help`}
                            checked={draftSounds}
                            onChange={(event) =>
                              setDraftSounds(event.target.checked)
                            }
                          />
                          <span>{playT('sounds')}</span>
                        </label>
                        <p
                          className="settings-help"
                          id={`${id}-play-sounds-help`}
                        >
                          {playT('soundsHelp')}
                        </p>
                      </div>
                      <div>
                        <label
                          className="settings-field"
                          htmlFor={`${id}-bearoff`}
                        >
                          <span>{playT('direction')}</span>
                          <select
                            id={`${id}-bearoff`}
                            aria-label={playT('direction')}
                            aria-describedby={`${id}-play-direction-help`}
                            value={draft.bearoff_direction ?? 'right'}
                            onChange={(event) =>
                              update({
                                bearoff_direction: event.target.value as
                                  'left' | 'right',
                              })
                            }
                          >
                            <option value="right">{playT('right')}</option>
                            <option value="left">{playT('left')}</option>
                          </select>
                        </label>
                        <p
                          className="settings-help"
                          id={`${id}-play-direction-help`}
                        >
                          {playT('directionHelp')}
                        </p>
                      </div>
                    </div>
                  </section>
                </fieldset>
                <fieldset
                  className="settings-section settings-board-section"
                  disabled={saving}
                  hidden={category !== 'board'}
                >
                  <legend>{t('board')}</legend>
                  <p className="settings-help settings-intro">
                    {t('boardHelp')}
                  </p>
                  <div className="settings-preview-layout">
                    <div
                      className="settings-theme-gallery"
                      role="group"
                      tabIndex={0}
                      aria-label={t('gallery')}
                    >
                      {themes.map((entry) => {
                        const archive = archiveThemes.find(
                          ({ id }) => id === entry.id,
                        );
                        return (
                          <button
                            key={entry.id}
                            type="button"
                            className="settings-theme-choice"
                            aria-label={entry.name}
                            aria-pressed={draftTheme === entry.id}
                            onClick={() => setDraftTheme(entry.id)}
                          >
                            <span
                              className="settings-theme-thumbnail"
                              aria-hidden="true"
                            >
                              {archive ? (
                                <>
                                  <img
                                    src={archive.background.src}
                                    alt=""
                                    loading="lazy"
                                  />
                                  <img
                                    className="settings-thumb-checker settings-thumb-light"
                                    src={archive.checkers.white[0].src}
                                    alt=""
                                    loading="lazy"
                                  />
                                  <img
                                    className="settings-thumb-checker settings-thumb-dark"
                                    src={archive.checkers.black[0].src}
                                    alt=""
                                    loading="lazy"
                                  />
                                </>
                              ) : (
                                <Board
                                  position={previewPosition}
                                  theme={entry.id}
                                  white=""
                                  black=""
                                />
                              )}
                              {draftTheme === entry.id && (
                                <span className="settings-selected">
                                  ✓ {t('selected')}
                                </span>
                              )}
                            </span>
                            <span className="settings-theme-name">
                              {entry.name}
                            </span>
                          </button>
                        );
                      })}
                    </div>
                    <section
                      className="settings-preview"
                      aria-label={t('preview', { theme: selectedTheme.name })}
                    >
                      <h3>{t('preview', { theme: selectedTheme.name })}</h3>
                      <Board
                        position={previewPosition}
                        theme={draftTheme}
                        white={t('light')}
                        black={t('dark')}
                      />
                    </section>
                  </div>
                </fieldset>
              </>
            )}
          </div>
        </div>
        <footer className="settings-footer">
          <div className="settings-feedback">
            {!destination && Boolean(saveError) && (
              <div className="settings-error" role="alert">
                <p>
                  {t(conflict ? 'conflict' : 'saveError')}{' '}
                  {!conflict && diagnostic(saveError)}
                </p>
                {conflict && (
                  <ActionButton
                    icon="refresh"
                    label={t('loadLatest')}
                    compact
                    type="button"
                    onClick={async () => {
                      if (configuration) await configuration.reloadServer();
                      setReload((value) => value + 1);
                    }}
                  />
                )}
              </div>
            )}
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
                label={
                  saving
                    ? t('saving')
                    : configuration
                      ? profileT('apply')
                      : t('save')
                }
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
          onSaved={onBookSaved}
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
          {Boolean(saveError) && (
            <p className="settings-error" role="alert">
              {t(conflict ? 'conflict' : 'saveError')}{' '}
              {!conflict && diagnostic(saveError)}
            </p>
          )}
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
