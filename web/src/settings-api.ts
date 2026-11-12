import type { components } from './generated-api';
import { isAnalysisDepth } from './analysis-depth';
import { isPlayMode, type PlayMode } from './play-mode-messages';

type Schema = components['schemas'];
export type AppSettings = Omit<
  Schema['AppSettings'],
  | 'analysis_engine'
  | 'play_engine'
  | 'bearoff_direction'
  | 'default_play_mode'
  | 'tutor_threshold'
  | 'training_auto_analysis'
> & {
  analysis_engine: 'gnubg' | 'bgsage';
  play_engine: 'gnubg' | 'bgsage';
  bearoff_direction?: 'right' | 'left';
  default_play_mode?: PlayMode;
  tutor_threshold?: number;
  training_auto_analysis?: boolean;
};
export type EngineInfo = Required<Schema['EngineInfo']>;

export class SettingsApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
    this.name = 'SettingsApiError';
  }
}

async function settingsRequest(path: string, options: RequestInit = {}) {
  const response = await fetch(`/api${path}`, options);
  let data: unknown;
  try {
    data = await response.json();
  } catch {
    throw new SettingsApiError(response.status, response.statusText);
  }
  if (!response.ok) {
    const diagnostic =
      data && typeof data === 'object' && 'error' in data
        ? String(data.error)
        : response.statusText;
    throw new SettingsApiError(response.status, diagnostic);
  }
  return data;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function parseSettings(value: unknown): AppSettings {
  if (
    !isObject(value) ||
    !Number.isInteger(value.version) ||
    typeof value.player_name !== 'string' ||
    !['gnubg', 'bgsage'].includes(String(value.analysis_engine)) ||
    !['gnubg', 'bgsage'].includes(String(value.play_engine)) ||
    (value.bearoff_direction !== undefined &&
      !['left', 'right'].includes(String(value.bearoff_direction))) ||
    (value.default_play_mode !== undefined &&
      !isPlayMode(value.default_play_mode)) ||
    (value.tutor_threshold !== undefined &&
      (typeof value.tutor_threshold !== 'number' ||
        !Number.isFinite(value.tutor_threshold) ||
        value.tutor_threshold <= 0 ||
        value.tutor_threshold > 1)) ||
    (value.training_auto_analysis !== undefined &&
      typeof value.training_auto_analysis !== 'boolean') ||
    !isAnalysisDepth(value.analysis_plies) ||
    ![0, 1, 2].includes(Number(value.play_plies)) ||
    typeof value.analysis_plies !== 'number' ||
    typeof value.play_plies !== 'number' ||
    typeof value.gnubg_path !== 'string' ||
    (value.analysis_engine === 'bgsage' && value.analysis_plies !== 2) ||
    (value.play_engine === 'bgsage' && value.play_plies !== 2)
  ) {
    throw new SettingsApiError(200, '');
  }
  return value as unknown as AppSettings;
}

export async function loadSettings(signal?: AbortSignal) {
  return parseSettings(await settingsRequest('/settings', { signal }));
}

export async function loadEngines(signal?: AbortSignal): Promise<EngineInfo[]> {
  const value = await settingsRequest('/engines', { signal });
  if (
    !Array.isArray(value) ||
    !value.every(
      (engine: unknown) =>
        isObject(engine) &&
        typeof engine.id === 'string' &&
        typeof engine.name === 'string' &&
        typeof engine.available === 'boolean' &&
        (engine.version === null || typeof engine.version === 'string') &&
        Array.isArray(engine.plies) &&
        engine.plies.every((ply: unknown) => Number.isInteger(ply)),
    )
  )
    throw new SettingsApiError(200, '');
  return value as EngineInfo[];
}

export async function saveSettings(value: AppSettings, signal?: AbortSignal) {
  return parseSettings(
    await settingsRequest('/settings', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(value),
      signal,
    }),
  );
}

export async function checkGnubg(path: string, signal?: AbortSignal) {
  const value = await settingsRequest('/engines/gnubg/check', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path }),
    signal,
  });
  if (
    !isObject(value) ||
    typeof value.version !== 'string' ||
    !value.version.trim()
  ) {
    throw new SettingsApiError(200, '');
  }
  return { version: value.version };
}

export type SageInstallation = Schema['Installation'];
function parseSageInstallation(value: unknown): SageInstallation {
  if (
    !isObject(value) ||
    !Number.isSafeInteger(value.version) ||
    typeof value.python_path !== 'string'
  )
    throw new SettingsApiError(200, 'Invalid Open Sage installation response');
  const snapshot = value.snapshot;
  if (
    snapshot != null &&
    (!isObject(snapshot) ||
      typeof snapshot.python !== 'string' ||
      typeof snapshot.package_version !== 'string' ||
      typeof snapshot.artifact_fingerprint !== 'string' ||
      snapshot.plies !== 2 ||
      typeof snapshot.threads !== 'number' ||
      ![2, 4].includes(snapshot.threads) ||
      snapshot.adapter_version !== 1)
  )
    throw new SettingsApiError(200, 'Invalid Open Sage installation identity');
  return value as SageInstallation;
}
export async function loadSageInstallation(signal?: AbortSignal) {
  return parseSageInstallation(
    await settingsRequest('/engines/bgsage', { signal }),
  );
}
export async function saveSageInstallation(
  version: number,
  path: string,
  signal?: AbortSignal,
) {
  return parseSageInstallation(
    await settingsRequest('/engines/bgsage', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ version, path }),
      signal,
    }),
  );
}
