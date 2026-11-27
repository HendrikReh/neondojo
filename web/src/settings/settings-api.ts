import configurationOptions from '../../../shared/configuration-options.json' with { type: 'json' };
import type { components } from '../generated-api';
import type { DepthEngine } from '../shared/engine/analysis-depth';
import { request } from '../shared/http/api';
import { AppError } from '../shared/i18n/i18n-core';
import {
  isPlayMode,
  type PlayMode,
} from '../workspaces/play/play-mode-messages';

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
export type EngineInfo = Schema['EngineInfo'] & { version: string | null };

import { isRecord as isObject } from '../shared/http/guards';

function isThreadLimit(value: unknown, engine: 'gnubg' | 'bgsage') {
  const { min } = configurationOptions.engine_threads[engine];
  return (
    value == null ||
    (typeof value === 'number' && Number.isSafeInteger(value) && value >= min)
  );
}

function isParallelJobLimit(value: unknown) {
  const { min } = configurationOptions.parallel_analysis_jobs;
  return (
    value == null ||
    (typeof value === 'number' && Number.isSafeInteger(value) && value >= min)
  );
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
    typeof value.analysis_plies !== 'number' ||
    typeof value.play_plies !== 'number' ||
    (value.play_top_two !== undefined &&
      typeof value.play_top_two !== 'boolean') ||
    (value.play_top_two === true &&
      (value.play_engine !== 'bgsage' || value.play_plies !== 2)) ||
    typeof value.gnubg_path !== 'string' ||
    !isThreadLimit(value.gnubg_threads, 'gnubg') ||
    !isThreadLimit(value.gnubg_play_threads, 'gnubg') ||
    !isThreadLimit(value.open_sage_threads, 'bgsage') ||
    !isThreadLimit(value.open_sage_play_threads, 'bgsage') ||
    !isParallelJobLimit(value.parallel_analysis_jobs) ||
    !configurationOptions.engine_profiles[
      value.analysis_engine as DepthEngine
    ].analysis.includes(value.analysis_plies) ||
    !configurationOptions.engine_profiles[
      value.play_engine as DepthEngine
    ].play.includes(value.play_plies)
  ) {
    throw new AppError('error.invalidResponse');
  }
  return value as unknown as AppSettings;
}

export async function loadSettings(signal?: AbortSignal) {
  return parseSettings(await request<unknown>('/settings', { signal }));
}

export type AppVersion = { release: string; commit: string };

export async function loadAppVersion(
  signal?: AbortSignal,
): Promise<AppVersion> {
  const value = await request<unknown>('/version', { signal });
  if (
    !isObject(value) ||
    typeof value.release !== 'string' ||
    typeof value.commit !== 'string'
  )
    throw new AppError('error.invalidResponse');
  return { release: value.release.trim(), commit: value.commit.trim() };
}

export async function loadEngines(signal?: AbortSignal): Promise<EngineInfo[]> {
  const value = await request<unknown>('/engines', { signal });
  if (
    !Array.isArray(value) ||
    !value.every(
      (engine: unknown) =>
        isObject(engine) &&
        typeof engine.id === 'string' &&
        typeof engine.name === 'string' &&
        typeof engine.available === 'boolean' &&
        (engine.version === null || typeof engine.version === 'string') &&
        (engine.max_parallel_threads == null ||
          (typeof engine.max_parallel_threads === 'number' &&
            Number.isSafeInteger(engine.max_parallel_threads) &&
            engine.max_parallel_threads > 0)) &&
        (engine.fork === undefined ||
          engine.fork === null ||
          (isObject(engine.fork) &&
            typeof engine.fork.repository === 'string' &&
            engine.fork.repository.length > 0 &&
            typeof engine.fork.revision === 'string' &&
            /^[0-9a-f]{40}$/.test(engine.fork.revision))) &&
        Array.isArray(engine.plies) &&
        engine.plies.every((ply: unknown) => Number.isInteger(ply)),
    )
  )
    throw new AppError('error.invalidResponse');
  return value as EngineInfo[];
}

export async function saveSettings(value: AppSettings, signal?: AbortSignal) {
  return parseSettings(
    await request<unknown>('/settings', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(value),
      signal,
    }),
  );
}

export async function checkGnubg(
  path: string,
  signal?: AbortSignal,
  threads?: number | null,
) {
  const value = await request<unknown>('/engines/gnubg/check', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, threads }),
    signal,
  });
  if (
    !isObject(value) ||
    typeof value.version !== 'string' ||
    !value.version.trim()
  ) {
    throw new AppError('error.invalidResponse');
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
    throw new AppError(
      'error.invalidResponse',
      {},
      'Invalid NeonGammon installation response',
    );
  const snapshot = value.snapshot;
  if (
    snapshot != null &&
    (!isObject(snapshot) ||
      typeof snapshot.python !== 'string' ||
      typeof snapshot.package_version !== 'string' ||
      typeof snapshot.artifact_fingerprint !== 'string' ||
      typeof snapshot.plies !== 'number' ||
      !configurationOptions.engine_profiles.bgsage.analysis.includes(
        snapshot.plies,
      ) ||
      (snapshot.one_ply_verified !== undefined &&
        typeof snapshot.one_ply_verified !== 'boolean') ||
      typeof snapshot.threads !== 'number' ||
      !isThreadLimit(snapshot.threads, 'bgsage') ||
      snapshot.adapter_version !== 1)
  )
    throw new AppError(
      'error.invalidResponse',
      {},
      'Invalid NeonGammon installation identity',
    );
  return value as SageInstallation;
}
export async function loadSageInstallation(signal?: AbortSignal) {
  return parseSageInstallation(
    await request<unknown>('/engines/bgsage', { signal }),
  );
}
export async function saveSageInstallation(
  version: number,
  path: string,
  signal?: AbortSignal,
) {
  return parseSageInstallation(
    await request<unknown>('/engines/bgsage', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ version, path }),
      signal,
    }),
  );
}
