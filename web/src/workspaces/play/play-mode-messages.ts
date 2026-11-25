import { defineMessages } from '../../shared/i18n/messages';
export const playModes = ['competition', 'training', 'coach', 'tutor'] as const;
export type PlayMode = (typeof playModes)[number];
export const isPlayMode = (value: unknown): value is PlayMode =>
  typeof value === 'string' && playModes.includes(value as PlayMode);

const messages = defineMessages('play-mode');
export const playModeText = messages.text;
export const usePlayModeText = messages.useText;

/** Every language's play-mode texts, for catalog completeness tests. */
export const playModeTextCatalogs = messages.catalogs;
