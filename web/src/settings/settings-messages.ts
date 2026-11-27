import { defineMessages, type MessageText } from '../shared/i18n/messages';
import { depthLabel } from '../shared/engine/analysis-depth';

export type SettingsMessageKey = keyof typeof messages.catalogs.en;

const messages = defineMessages('settings');

export const settingsMessages = messages.catalogs;
export const settingsText = messages.text;
export const useSettingsText = messages.useText;

export type SettingsText = MessageText<SettingsMessageKey>;

/** GNUbg's three named profiles are its 1- to 3-ply searches. */
export function depthOptionLabel(
  t: SettingsText,
  ply: number,
  engine: string,
  names: readonly [SettingsMessageKey, SettingsMessageKey, SettingsMessageKey],
) {
  const depth = depthLabel(ply, engine);
  const name = engine === 'gnubg' ? names[ply - 1] : undefined;
  return name ? t(name, { depth }) : depth;
}

/** Option text for a bot's playing strength. */
export const playStrengthLabel = (
  t: SettingsText,
  ply: number,
  engine: string,
  topTwo = false,
) =>
  topTwo
    ? '2-ply + Top-2 (10 mEq)'
    : depthOptionLabel(t, ply, engine, [
        'fast',
        'strengthStronger',
        'strengthStrongest',
      ]);
