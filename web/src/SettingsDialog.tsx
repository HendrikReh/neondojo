import { ANALYSIS_PLIES, isAnalysisDepth } from './analysis-depth';
import { useEffect, useId, useRef, useState, type FormEvent } from 'react';
import Board from './Board';
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
import { useSourceText } from './analysis-source-messages';
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
  onClose: () => void;
  onBookSettings: () => void;
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

type Category = 'general' | 'appearance' | 'analysis' | 'play' | 'board';
type CheckState =
  { path: string; version: string } | { path: string; error: unknown } | null;

export default function SettingsDialog({
  open,
  onClose,
  theme,
  appearance,
  onApply,
  onBookSettings,
}: SettingsDialogProps) {
  const { locale } = useI18n();
  const t = useSettingsText();
  const sourceT = useSourceText();
  const appearanceT = useUiAppearanceText();
  const id = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const preferences = useRef({ theme, locale, appearance });
  preferences.current = { theme, locale, appearance };
  const [category, setCategory] = useState<Category>('general');
  const [draftTheme, setDraftTheme] = useState(theme);
  const [draftLocale, setDraftLocale] = useState(locale);
  const [draftAppearance, setDraftAppearance] = useState(appearance);
  const [draft, setDraft] = useState<AppSettings | null>(null);
  const [initial, setInitial] = useState<AppSettings | null>(null);
  const [engines, setEngines] = useState<EngineInfo[]>([]);
  const [reload, setReload] = useState(0);
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
    setCategory('general');
    setDraftTheme(preferences.current.theme);
    setDraftLocale(preferences.current.locale);
    setDraftAppearance(preferences.current.appearance);
    element.showModal();
    document.body.style.overflow = 'hidden';
    return () => {
      element.close();
      document.body.style.overflow = bodyOverflow;
      if (active?.isConnected) active.focus();
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const currentSession = ++session.current;
    const abort = new AbortController();
    setLoading(true);
    setDraft(null);
    setInitial(null);
    setEngines([]);
    setLoadError(null);
    setSaveError(null);
    setCheck(null);
    setChecking(false);
    setSaving(false);
    savingRef.current = false;
    Promise.all([loadSettings(abort.signal), loadEngines(abort.signal)])
      .then(([settings, catalog]) => {
        if (abort.signal.aborted || session.current !== currentSession) return;
        setDraft(settings);
        setInitial(settings);
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

  const dirty = Boolean(
    draft &&
    initial &&
    (JSON.stringify(draft) !== JSON.stringify(initial) ||
      draftTheme !== theme ||
      draftAppearance.theme !== appearance.theme ||
      draftAppearance.mode !== appearance.mode ||
      draftLocale !== locale),
  );
  const conflict =
    saveError instanceof SettingsApiError && saveError.status === 409;
  const engine = engines.find(({ id }) => id === 'gnubg');
  const selectedTheme = themes.find(({ id }) => id === draftTheme) ?? themes[0];
  const plies = engine?.plies.filter(isAnalysisDepth) ?? [];

  function diagnostic(error: unknown) {
    if (error instanceof SettingsApiError && error.status === 200)
      return t('invalidResponse');
    return error instanceof Error ? error.message : '';
  }

  function update(values: Partial<AppSettings>) {
    setDraft((previous) => (previous ? { ...previous, ...values } : previous));
    if (!conflict) setSaveError(null);
  }

  function close() {
    if (!savingRef.current) onClose();
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
      const result = await checkGnubg(path, abort.signal);
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

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!draft || loading || checking || conflict || savingRef.current) return;
    savingRef.current = true;
    setSaving(true);
    setSaveError(null);
    const abort = new AbortController();
    saveAbort.current = abort;
    const currentSession = session.current;
    let settings: AppSettings;
    try {
      settings = await saveSettings(draft, abort.signal);
    } catch (error) {
      if (!abort.signal.aborted && session.current === currentSession) {
        setSaveError(error);
        setSaving(false);
        savingRef.current = false;
      }
      return;
    }
    if (abort.signal.aborted || session.current !== currentSession) return;
    saveTheme(draftTheme);
    saveLocale(draftLocale);
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
    onClose();
  }

  function engineSelect(label: SettingsMessageKey, name: string) {
    return (
      <label className="settings-field" htmlFor={`${id}-${name}`}>
        <span>{t(label)}</span>
        <select
          aria-label={t(label)}
          id={`${id}-${name}`}
          value="gnubg"
          onChange={() => {}}
        >
          <option value="gnubg">
            {engine?.name ?? 'GNU Backgammon'}
            {engine?.available ? '' : ` — ${t('unavailable')}`}
          </option>
          <option value="bgsage" disabled>
            {t('planned')}
          </option>
        </select>
      </label>
    );
  }

  return (
    <dialog
      ref={dialog}
      className="settings-dialog"
      aria-labelledby={`${id}-title`}
      aria-describedby={`${id}-description`}
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key !== 'Tab') return;
        const focusable = [
          ...event.currentTarget.querySelectorAll<HTMLElement>(
            'button,input,select,textarea,a[href]',
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
          <button
            type="button"
            className="settings-close"
            aria-label={t('close')}
            onClick={close}
            disabled={saving}
            autoFocus
          >
            ×
          </button>
        </header>
        <div className="settings-layout">
          <nav className="settings-categories" aria-label={t('categories')}>
            {(
              ['general', 'appearance', 'analysis', 'play', 'board'] as const
            ).map((name) => (
              <button
                key={name}
                type="button"
                aria-current={category === name ? 'page' : undefined}
                className={category === name ? 'active' : ''}
                onClick={() => setCategory(name)}
                disabled={saving}
              >
                {name === 'appearance' ? appearanceT('appearance') : t(name)}
              </button>
            ))}
          </nav>
          <div className="settings-content" aria-busy={loading}>
            {loading && <p role="status">{t('loading')}</p>}
            {Boolean(loadError) && (
              <div className="settings-error" role="alert">
                <p>
                  {t('loadError')} {diagnostic(loadError)}
                </p>
                <button
                  type="button"
                  onClick={() => setReload((value) => value + 1)}
                >
                  {t('retry')}
                </button>
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
                      {(['light', 'dark', 'system'] as const).map((mode) => (
                        <option key={mode} value={mode}>
                          {appearanceT(mode)}
                        </option>
                      ))}
                    </select>
                  </label>
                  <p className="settings-help" id={`${id}-system-help`}>
                    {appearanceT('systemHelp')}
                  </p>
                  <UiAppearancePreview appearance={draftAppearance} />
                </fieldset>
                <fieldset
                  className="settings-section"
                  disabled={saving}
                  hidden={category !== 'analysis'}
                >
                  <legend>{t('analysis')}</legend>
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
                          disabled={!plies.includes(ply)}
                        >
                          {ply === 3 || ply === 4
                            ? `${ply}-ply`
                            : t((['fast', 'deep', 'thorough'] as const)[ply])}
                        </option>
                      ))}
                    </select>
                  </label>
                  <p className="settings-help">{t('analysisHelp')}</p>
                  <button
                    type="button"
                    className="settings-book-button"
                    onClick={() => {
                      onBookSettings();
                    }}
                  >
                    {sourceT('books')}
                  </button>
                  <h3 className="settings-subheading">{t('installation')}</h3>
                  <label className="settings-field" htmlFor={`${id}-path`}>
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
                        checkAbort.current?.abort();
                        setChecking(false);
                        setCheck(null);
                        update({ gnubg_path: event.target.value });
                      }}
                    />
                  </label>
                  <p id={`${id}-path-help`} className="settings-help">
                    {t('pathHelp')}
                  </p>
                  <div
                    className="settings-engine-status"
                    id={`${id}-check-status`}
                  >
                    {engine?.version &&
                      !check &&
                      draft.gnubg_path === initial?.gnubg_path && (
                        <p>{t('engineVersion', { version: engine.version })}</p>
                      )}
                    {!engine?.available && <p>{t('unavailable')}</p>}
                    {check && 'version' in check && (
                      <p role="status" className="settings-success">
                        {t('checkSuccess', { version: check.version })}
                      </p>
                    )}
                    {check && 'error' in check && (
                      <p role="alert" className="settings-error">
                        {t('checkError')} {diagnostic(check.error)}
                      </p>
                    )}
                    {!check && draft.gnubg_path !== initial?.gnubg_path && (
                      <p>{t('checkNeeded')}</p>
                    )}
                    {checking && <p role="status">{t('checking')}</p>}
                  </div>
                  <button
                    type="button"
                    className="settings-check"
                    disabled={checking || !draft.gnubg_path.trim()}
                    onClick={checkInstallation}
                  >
                    {t('check')}
                  </button>
                  <p className="settings-help">{t('checkHelp')}</p>
                </fieldset>
                <fieldset
                  className="settings-section"
                  disabled={saving}
                  hidden={category !== 'play'}
                >
                  <legend>{t('play')}</legend>
                  <p className="settings-help settings-intro">
                    {t('playHelp')}
                  </p>
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
                          disabled={!plies.includes(ply)}
                        >
                          {t(
                            (['strength0', 'strength1', 'strength2'] as const)[
                              ply
                            ],
                          )}
                        </option>
                      ))}
                    </select>
                  </label>
                  <p className="settings-help">{t('playInstallation')}</p>
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
                  <div
                    className="settings-theme-gallery"
                    role="group"
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
                          </span>
                          <span className="settings-theme-name">
                            {entry.name}
                          </span>
                          {draftTheme === entry.id && (
                            <span className="settings-selected">
                              ✓ {t('selected')}
                            </span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                </fieldset>
              </>
            )}
          </div>
        </div>
        <footer className="settings-footer">
          <div className="settings-feedback">
            {Boolean(saveError) && (
              <div className="settings-error" role="alert">
                <p>
                  {t(conflict ? 'conflict' : 'saveError')}{' '}
                  {!conflict && diagnostic(saveError)}
                </p>
                {conflict && (
                  <button
                    type="button"
                    onClick={() => {
                      setDraftTheme(theme);
                      setDraftLocale(locale);
                      setDraftAppearance(appearance);
                      setReload((value) => value + 1);
                    }}
                  >
                    {t('loadLatest')}
                  </button>
                )}
              </div>
            )}
            {dirty && !saveError && (
              <p className="settings-help">{t('unsaved')}</p>
            )}
          </div>
          <div className="settings-actions">
            <button type="button" onClick={close} disabled={saving}>
              {t('cancel')}
            </button>
            <button
              type="submit"
              className="settings-save"
              disabled={!draft || loading || saving || checking || conflict}
            >
              {t(saving ? 'saving' : 'save')}
            </button>
          </div>
        </footer>
      </form>
    </dialog>
  );
}
