import type { ProfileConfiguration } from './configuration-model';
import type { AppSettings } from './settings-api';
import { loadSettings, saveSettings, SettingsApiError } from './settings-api';
import type { Locale } from './i18n-core';
import type { UiAppearance } from './ui-appearance';

export async function saveInstallationPath(
  initial: AppSettings,
  path: string,
  signal?: AbortSignal,
) {
  const latest = await loadSettings(signal);
  if (latest.gnubg_path !== initial.gnubg_path) {
    throw new SettingsApiError(
      409,
      'The engine installation changed; reload before applying a new path',
    );
  }
  return saveSettings({ ...latest, gnubg_path: path }, signal);
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

export function applySettingsDraft(
  current: ProfileConfiguration,
  draft: {
    settings: AppSettings;
    locale: Locale;
    theme: string;
    appearance: UiAppearance;
    sounds: boolean;
  },
): ProfileConfiguration {
  const { settings } = draft;
  return {
    ...current,
    general: {
      player_name: settings.player_name,
      locale: draft.locale,
      play_sounds: draft.sounds,
    },
    analysis: {
      ...current.analysis,
      engine: settings.analysis_engine,
      plies: settings.analysis_plies,
    },
    play: {
      ...current.play,
      engine: settings.play_engine,
      plies: settings.play_plies,
      default_mode: settings.default_play_mode ?? 'coach',
      tutor_threshold: settings.tutor_threshold ?? 0.04,
      training_auto_analysis: settings.training_auto_analysis ?? true,
    },
    appearance: {
      ...current.appearance,
      ui_theme: draft.appearance.theme,
      mode: draft.appearance.mode,
      board_theme: draft.theme,
      bearoff_direction: settings.bearoff_direction ?? 'right',
    },
  };
}
