import { useEffect, useId, useRef, useState, type FormEvent } from 'react';
import configurationOptions from '../../../shared/configuration-options.json' with { type: 'json' };
import LazyPanel from '../app/LazyPanel';
import ActionButton from '../shared/ui/ActionButton';
import ModalDialog from '../shared/ui/ModalDialog';
import { useRovingTabs } from '../shared/ui/TabList';
import SettingsDraftGuardDialog, {
  type GuardedDestination,
} from './SettingsDraftGuardDialog';
import { preferredDepth } from '../shared/engine/analysis-depth';
import type { Theme } from '../shared/board/themes';
import type { UiAppearance } from './appearance/ui-appearance';
import { useUiAppearanceText } from './appearance/ui-appearance-messages';
import { useProfileText } from './profiles/profile-messages';
import ProfileSettingsPanel from './profiles/ProfileSettingsPanel';
import OpeningBookDialog from './OpeningBookDialog';
import GeneralSettingsSection from './GeneralSettingsSection';
import AppearanceSettingsSection from './AppearanceSettingsSection';
import AnalysisSettingsSection from './AnalysisSettingsSection';
import PlaySettingsSection from './PlaySettingsSection';
import BoardSettingsSection from './BoardSettingsSection';
import EngineSettingsSection from './EngineSettingsSection';
import { useSettingsText, type SettingsMessageKey } from './settings-messages';
import {
  useSettingsDraft,
  type SettingsApplyPayload,
} from './useSettingsDraft';
import './settings.css';

export interface SettingsDialogProps {
  open: boolean;
  initialCategory?: 'general' | 'play' | 'engines' | 'profiles';
  onClose: () => void;
  onBookRefresh: () => void;
  onBenchmark?: () => void;
  theme: Theme;
  appearance: UiAppearance;
  onApply: (value: SettingsApplyPayload) => void;
}

type Category =
  | 'general'
  | 'appearance'
  | 'analysis'
  | 'play'
  | 'board'
  | 'engines'
  | 'profiles';

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
  const id = useId();
  const t = useSettingsText();
  const profileT = useProfileText();
  const appearanceT = useUiAppearanceText();
  const [category, setCategory] = useState<Category>('general');
  const [destination, setDestination] = useState<GuardedDestination | null>(
    null,
  );
  const [bookOpen, setBookOpen] = useState(false);
  const bookTrigger = useRef<HTMLButtonElement>(null);
  const dialogTrigger = useRef<HTMLElement | null>(null);

  const state = useSettingsDraft({
    open,
    inProfilesSection: category === 'profiles',
    theme,
    appearance,
    onApply,
    onReloadCleared: () => setDestination(null),
  });

  const categories: Category[] = [
    'general',
    'appearance',
    'analysis',
    'play',
    'board',
    'engines',
  ];
  if (state.configuration) categories.push('profiles');
  const applyLabel = state.configuration ? profileT('apply') : t('save');

  useEffect(() => {
    if (destination || bookOpen) return;
    if (open) dialogTrigger.current?.focus();
    dialogTrigger.current = null;
  }, [destination, bookOpen, open]);

  function continueTo(next: GuardedDestination) {
    dialogTrigger.current = null;
    setDestination(null);
    if (next === 'profiles') {
      if (destination === 'profiles')
        dialogTrigger.current = categoryNav.getTabElement('profiles');
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
    if (state.savingRef.current || state.sage.busy) return;
    if (state.dirty) {
      dialogTrigger.current =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
      setDestination(next);
    } else continueTo(next);
  }

  function enterSection(next: Category) {
    const current = state.configurationRef.current?.current;
    if (category === 'profiles' && next !== 'profiles' && current)
      state.rebaseFromProfile(current);
    if (next === 'profiles') requestDestination(next);
    else setCategory(next);
  }

  const categoryNav = useRovingTabs<Category>({
    items: categories,
    active: category,
    orientation: 'vertical',
    wrap: false,
    onSelect: enterSection,
    shouldFocus: (next) => next !== 'profiles' || !state.dirty,
  });

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await state.applyDraft();
  }

  const saveFeedback = Boolean(state.saveError) && (
    <div className="settings-error" role="alert">
      <p>
        {t(state.conflict ? 'conflict' : 'saveError')}{' '}
        {!state.conflict && state.diagnostic(state.saveError)}
      </p>
      {Boolean(state.recoveryError) && (
        <p>
          {t('loadError')} {state.diagnostic(state.recoveryError)}
        </p>
      )}
      {state.conflict && (
        <ActionButton
          icon="refresh"
          label={t('loadLatest')}
          type="button"
          disabled={state.saving}
          onClick={() => void state.reloadLatest()}
        />
      )}
    </div>
  );

  function engineSelect(label: SettingsMessageKey, name: string) {
    const area = name === 'analysis-engine' ? 'analysis' : 'play';
    const selected =
      area === 'analysis'
        ? state.draft?.analysis_engine
        : state.draft?.play_engine;
    return (
      <label className="settings-field" htmlFor={`${id}-${name}`}>
        <span>{t(label)}</span>
        <select
          aria-label={t(label)}
          id={`${id}-${name}`}
          value={selected}
          onChange={(event) => {
            const value = event.target.value;
            if (!state.draft || (value !== 'gnubg' && value !== 'bgsage'))
              return;
            state.rememberedDepths.current[`${area}-${selected}`] =
              area === 'analysis'
                ? state.draft.analysis_plies
                : state.draft.play_plies;
            const options = configurationOptions.engine_profiles[value][area];
            const remembered =
              state.rememberedDepths.current[`${area}-${value}`] ??
              preferredDepth(value);
            const depth = options.includes(remembered)
              ? remembered
              : options[0];
            state.update(
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
          {state.engines.map((item) => (
            <option key={item.id} value={item.id} disabled={!item.available}>
              {item.name}
              {item.available ? '' : ` — ${t('unavailable')}`}
            </option>
          ))}
        </select>
      </label>
    );
  }

  return (
    <>
      <ModalDialog
        open={open}
        className="settings-dialog"
        ariaLabelledBy={`${id}-title`}
        ariaDescribedBy={`${id}-description`}
        onOpen={() => {
          setCategory(initialCategory);
          setDestination(null);
          state.resetPreferences(state.configurationRef.current?.current);
        }}
        onCancel={() => requestDestination('close')}
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
              disabled={state.saving || state.sage.busy}
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
                  ref={categoryNav.getTabRef(name)}
                  type="button"
                  aria-current={category === name ? 'page' : undefined}
                  className={category === name ? 'active' : ''}
                  onClick={() => enterSection(name)}
                  onKeyDown={(event) => categoryNav.onKeyDown(event, name)}
                  disabled={state.saving || state.sage.busy}
                >
                  {name === 'profiles'
                    ? profileT('profiles')
                    : name === 'appearance'
                      ? appearanceT('appearance')
                      : t(name)}
                </button>
              ))}
            </nav>
            <div className="settings-content" aria-busy={state.loading}>
              {category === 'profiles' && <ProfileSettingsPanel />}
              {state.loading && category !== 'profiles' && (
                <p role="status">{t('loading')}</p>
              )}
              {category !== 'profiles' && Boolean(state.loadError) && (
                <div className="settings-error" role="alert">
                  <p>
                    {t('loadError')} {state.diagnostic(state.loadError)}
                  </p>
                  <ActionButton
                    icon="refresh"
                    label={t('retry')}
                    compact
                    type="button"
                    onClick={state.triggerReload}
                  />
                </div>
              )}
              {state.draft && (
                <>
                  <GeneralSettingsSection
                    id={id}
                    disabled={state.saving}
                    hidden={category !== 'general'}
                    applyLabel={applyLabel}
                    draftLocale={state.draftLocale}
                    onLocale={state.setDraftLocale}
                    playerName={state.draft.player_name}
                    onPlayerName={(player_name) => state.update({ player_name })}
                    myProfile={state.myProfile}
                  />
                  <AppearanceSettingsSection
                    id={id}
                    disabled={state.saving}
                    hidden={category !== 'appearance'}
                    applyLabel={applyLabel}
                    appearance={state.draftAppearance}
                    onChange={state.setDraftAppearance}
                  />
                  <AnalysisSettingsSection
                    id={id}
                    disabled={state.saving}
                    hidden={category !== 'analysis'}
                    draft={state.draft}
                    analysisPlies={state.analysisPlies}
                    engineField={engineSelect(
                      'analysisEngine',
                      'analysis-engine',
                    )}
                    hasConfiguration={Boolean(state.configuration)}
                    openingBookProvider={
                      state.configuration?.current?.opening_book.provider
                    }
                    thresholdFields={state.thresholdFields}
                    onThresholdChange={(field, value) => {
                      state.setThresholdFields({
                        ...state.thresholdFields,
                        [field]: value,
                      });
                      state.setSaveError(null);
                    }}
                    onUpdate={state.update}
                    bookTriggerRef={bookTrigger}
                    onManageBooks={() => requestDestination('book')}
                    onBenchmark={
                      onBenchmark
                        ? () => requestDestination('benchmark')
                        : undefined
                    }
                  />
                  <PlaySettingsSection
                    id={id}
                    draft={state.draft}
                    update={state.update}
                    disabled={state.saving}
                    hidden={category !== 'play'}
                    engineField={engineSelect('playEngine', 'play-engine')}
                    sounds={state.draftSounds}
                    onSounds={state.setDraftSounds}
                  />
                  {open && category === 'board' && (
                    <LazyPanel>
                      <BoardSettingsSection
                        disabled={state.saving}
                        artwork={state.draftBoard}
                        onChange={state.setDraftBoard}
                      />
                    </LazyPanel>
                  )}
                  <EngineSettingsSection
                    id={id}
                    draft={state.draft}
                    update={state.update}
                    disabled={state.saving}
                    hidden={category !== 'engines'}
                    engines={state.engines}
                    engineCatalogRefresh={state.engineCatalogRefresh}
                    onRefreshEngines={() => void state.refreshEngines()}
                    check={state.check}
                    checking={state.checking}
                    gnubgEdited={state.gnubgEdited}
                    onCheck={state.checkInstallation}
                    onInstallationEdited={state.resetInstallationCheck}
                    diagnostic={state.diagnostic}
                    sage={state.sage}
                  />
                </>
              )}
            </div>
          </div>
          <footer className="settings-footer">
            <div className="settings-feedback">
              {!destination && saveFeedback}
              {state.dirty && (!state.saveError || destination) && (
                <p className="settings-help" role="status">
                  {t('unsaved')}
                </p>
              )}
              {!!state.applyHint && (
                <p id={`${id}-apply-target`} className="settings-help">
                  {state.applyHint}
                </p>
              )}
            </div>
            <div className="settings-actions">
              <ActionButton
                icon="close"
                label={t('done')}
                type="button"
                onClick={() => requestDestination('close')}
                disabled={state.saving || state.sage.busy}
              />
              {category !== 'profiles' && (state.dirty || state.saving) && (
                <ActionButton
                  icon="save"
                  label={state.saving ? t('saving') : applyLabel}
                  type="submit"
                  className="settings-save"
                  aria-describedby={
                    state.applyHint ? `${id}-apply-target` : undefined
                  }
                  disabled={!state.canApply}
                />
              )}
            </div>
          </footer>
        </form>
      </ModalDialog>
      {open && bookOpen && (
        <OpeningBookDialog
          onClose={() => setBookOpen(false)}
          onRefresh={onBookRefresh}
        />
      )}
      {open && destination && (
        <SettingsDraftGuardDialog
          id={id}
          destination={destination}
          saving={state.saving}
          canApply={state.canApply}
          applyHint={state.applyHint}
          saveFeedback={saveFeedback}
          onCancel={() => setDestination(null)}
          onDiscard={() => {
            state.discardDraft();
            continueTo(destination);
          }}
          onApplyAndContinue={async () => {
            if (await state.applyDraft()) continueTo(destination);
          }}
        />
      )}
    </>
  );
}
