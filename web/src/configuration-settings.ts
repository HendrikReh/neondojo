import {
  defaultConfiguration,
  type ProfileConfiguration,
} from './configuration-model';
import type { AppSettings } from './settings-api';
import { loadSettings, saveSettings, SettingsApiError } from './settings-api';
import type { Locale } from './i18n-core';
import type { UiAppearance } from './ui-appearance';

const installationFields = [
  'gnubg_path',
  'gnubg_threads',
  'open_sage_threads',
] as const;

export function changedInstallationFields(
  initial: AppSettings,
  draft: AppSettings,
) {
  return installationFields.filter(
    (field) => (initial[field] ?? null) !== (draft[field] ?? null),
  );
}

export async function saveInstallationSettings(
  initial: AppSettings,
  draft: AppSettings,
  signal?: AbortSignal,
) {
  const changed = changedInstallationFields(initial, draft);
  if (!changed.length) return;
  const latest = await loadSettings(signal);
  const edits = changed.filter(
    (field) => (latest[field] ?? null) !== (draft[field] ?? null),
  );
  if (
    edits.some((field) => (latest[field] ?? null) !== (initial[field] ?? null))
  ) {
    throw new SettingsApiError(
      409,
      'The local engine settings changed; reload before applying your changes',
    );
  }
  if (!edits.length) return latest;
  return saveSettings(
    {
      ...latest,
      ...Object.fromEntries(edits.map((field) => [field, draft[field]])),
    },
    signal,
  );
}

export function settingsFromConfiguration(
  configuration: ProfileConfiguration,
  installation: AppSettings,
): AppSettings {
  return {
    ...installation,
    player_name: configuration.general.player_name,
    analysis_engine: configuration.analysis.engine,
    analysis_plies: configuration.analysis.plies,
    play_engine: configuration.play.engine,
    play_plies: configuration.play.plies,
    default_play_mode: configuration.play.default_mode,
    tutor_threshold: configuration.play.tutor_threshold,
    training_auto_analysis: configuration.play.training_auto_analysis,
    bearoff_direction: configuration.appearance.bearoff_direction,
  };
}

interface SettingsDraft {
  settings: AppSettings;
  locale: Locale;
  theme: string;
  appearance: UiAppearance;
  sounds: boolean;
}

function profileSettings(draft: SettingsDraft) {
  const { settings } = draft;
  return {
    general: {
      player_name: settings.player_name,
      locale: draft.locale,
      play_sounds: draft.sounds,
    },
    analysis: {
      engine: settings.analysis_engine,
      plies: settings.analysis_plies,
    },
    play: {
      engine: settings.play_engine,
      plies: settings.play_plies,
      default_mode:
        settings.default_play_mode ?? defaultConfiguration.play.default_mode,
      tutor_threshold:
        settings.tutor_threshold ?? defaultConfiguration.play.tutor_threshold,
      training_auto_analysis:
        settings.training_auto_analysis ??
        defaultConfiguration.play.training_auto_analysis,
    },
    appearance: {
      ui_theme: draft.appearance.theme,
      mode: draft.appearance.mode,
      board_theme: draft.theme,
      bearoff_direction:
        settings.bearoff_direction ??
        defaultConfiguration.appearance.bearoff_direction,
    },
  };
}

function changedValues<T extends object>(values: T, initial?: T): Partial<T> {
  if (!initial) return values;
  const changes: Partial<T> = {};
  for (const key in values) {
    if (values[key] !== initial[key]) changes[key] = values[key];
  }
  return changes;
}

export function applySettingsDraft(
  current: ProfileConfiguration,
  draft: SettingsDraft,
  initial?: SettingsDraft,
): ProfileConfiguration {
  const values = profileSettings(draft);
  const baseline = initial && profileSettings(initial);
  const analysis = changedValues(values.analysis, baseline?.analysis);
  const play = changedValues(values.play, baseline?.play);
  // Engine and depth form one setting because supported depths depend on the engine.
  if (analysis.engine !== undefined || analysis.plies !== undefined)
    Object.assign(analysis, values.analysis);
  if (play.engine !== undefined || play.plies !== undefined) {
    play.engine = values.play.engine;
    play.plies = values.play.plies;
  }
  return {
    ...current,
    general: {
      ...current.general,
      ...changedValues(values.general, baseline?.general),
    },
    analysis: { ...current.analysis, ...analysis },
    play: { ...current.play, ...play },
    appearance: {
      ...current.appearance,
      ...changedValues(values.appearance, baseline?.appearance),
    },
  };
}
