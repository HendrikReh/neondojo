import type { components } from './generated-api';
import { isAnalysisDepth } from './analysis-depth';

type Schema = components['schemas'];
export type AppSettings = Omit<
  Schema['AppSettings'],
  'analysis_engine' | 'play_engine'
> & {
  analysis_engine: 'gnubg';
  play_engine: 'gnubg';
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
    value.analysis_engine !== 'gnubg' ||
    value.play_engine !== 'gnubg' ||
    !isAnalysisDepth(value.analysis_plies) ||
    ![0, 1, 2].includes(Number(value.play_plies)) ||
    typeof value.analysis_plies !== 'number' ||
    typeof value.play_plies !== 'number' ||
    typeof value.gnubg_path !== 'string'
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
