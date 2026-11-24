import { ConflictError, request, type ErrorIssue } from '../../shared/http/api';
import { AppError } from '../../shared/i18n/i18n-core';
import {
  normalizeConfiguration,
  isConfigurationDirty,
  type ProfileConfiguration,
  type ConfigurationDifference,
} from './configuration-model';

export interface ConfigurationProfile {
  id: string;
  name: string;
  revision: number;
  configuration: ProfileConfiguration;
  created_at?: string;
  updated_at?: string;
}
export interface ConfigurationState {
  revision: number;
  active_profile: ConfigurationProfile;
  current_configuration: ProfileConfiguration;
  dirty: boolean;
  differences: ConfigurationDifference[];
  profile?: ConfigurationProfile;
  initialization_warnings?: ConfigurationIssue[];
}
export type ConfigurationIssue = ErrorIssue;

/** A conflict found while saving or recovering the configuration. */
export class ConfigurationConflict extends ConflictError {}

/**
 * Whether the server or this client decided the outcome. After a lost
 * connection the server may still have applied the request.
 */
export const isDecided = (error: unknown) =>
  error instanceof AppError && error.key !== 'error.network';

const object = (v: unknown): v is Record<string, unknown> =>
  !!v && typeof v === 'object' && !Array.isArray(v);
export function parseAvailability(value: unknown): {
  revision: number;
  valid: boolean;
  issues: ConfigurationIssue[];
} {
  if (
    !object(value) ||
    !Number.isSafeInteger(value.revision) ||
    Number(value.revision) < 1 ||
    typeof value.valid !== 'boolean' ||
    !Array.isArray(value.issues) ||
    value.valid !== (value.issues.length === 0) ||
    value.issues.some(
      (issue) =>
        !object(issue) ||
        typeof issue.path !== 'string' ||
        typeof issue.code !== 'string' ||
        typeof issue.message !== 'string',
    )
  )
    throw new AppError(
      'error.invalidResponse',
      {},
      'Invalid configuration availability response',
    );
  return value as unknown as {
    revision: number;
    valid: boolean;
    issues: ConfigurationIssue[];
  };
}
export function parseProfile(value: unknown): ConfigurationProfile {
  if (
    !object(value) ||
    typeof value.id !== 'string' ||
    !/^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i.test(value.id) ||
    typeof value.name !== 'string' ||
    !value.name.trim() ||
    !Number.isSafeInteger(value.revision) ||
    Number(value.revision) < 1
  )
    throw new AppError('error.invalidResponse', {}, 'Invalid profile response');
  return {
    ...value,
    configuration: normalizeConfiguration(value.configuration),
  } as unknown as ConfigurationProfile;
}
export function parseConfigurationState(value: unknown): ConfigurationState {
  if (
    !object(value) ||
    !Number.isSafeInteger(value.revision) ||
    Number(value.revision) < 1 ||
    typeof value.dirty !== 'boolean' ||
    !Array.isArray(value.differences)
  )
    throw new AppError(
      'error.invalidResponse',
      {},
      'Invalid configuration response',
    );
  const profile = parseProfile(value.active_profile),
    current = normalizeConfiguration(value.current_configuration);
  if (
    value.dirty !== isConfigurationDirty(profile.configuration, current) ||
    value.differences.some(
      (d) =>
        !object(d) ||
        typeof d.path !== 'string' ||
        !Object.hasOwn(d, 'saved') ||
        !Object.hasOwn(d, 'current'),
    )
  )
    throw new AppError(
      'error.invalidResponse',
      {},
      'Invalid configuration differences',
    );
  return {
    ...value,
    active_profile: profile,
    current_configuration: current,
  } as unknown as ConfigurationState;
}
export const configurationRequest = (
  path: string,
  method = 'GET',
  body?: unknown,
) =>
  request<unknown>(path, {
    method,
    ...(body === undefined
      ? {}
      : {
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        }),
  });
export interface ConfigurationApi {
  read(): Promise<ConfigurationState>;
  initialize(): Promise<ConfigurationState>;
  listProfiles(): Promise<ConfigurationProfile[]>;
  write(
    revision: number,
    configuration: ProfileConfiguration,
  ): Promise<ConfigurationState>;
  mutate(
    path: string,
    method: string,
    body: unknown,
  ): Promise<ConfigurationState>;
  availability?(): Promise<{
    revision: number;
    valid: boolean;
    issues: ConfigurationIssue[];
  }>;
}
export const configurationApi: ConfigurationApi = {
  read: async () =>
    parseConfigurationState(await configurationRequest('/configuration')),
  initialize: async () => {
    const [{ loadSettings }, { bookSettings }, { readLegacyConfiguration }] =
      await Promise.all([
        import('../settings-api'),
        import('../../workspaces/analysis/candidates/source-analysis-api'),
        import('./configuration-legacy'),
      ]);
    const controller = new AbortController();
    const [settings, book] = await Promise.all([
      loadSettings(controller.signal),
      bookSettings(controller.signal),
    ]);
    return parseConfigurationState(
      await configurationRequest('/configuration/initialize', 'POST', {
        expected_settings_version: settings.version,
        expected_book_version: book.config.version,
        configuration: readLegacyConfiguration(settings, book.config),
      }),
    );
  },
  listProfiles: async () => {
    const value = await configurationRequest('/profiles');
    if (!Array.isArray(value))
      throw new AppError('error.invalidResponse', {}, 'Invalid profile list');
    return value.map(parseProfile);
  },
  write: async (revision, configuration) =>
    parseConfigurationState(
      await configurationRequest('/configuration', 'PUT', {
        expected_revision: revision,
        configuration,
      }),
    ),
  mutate: async (path, method, body) =>
    parseConfigurationState(await configurationRequest(path, method, body)),
  availability: async () =>
    parseAvailability(
      await configurationRequest('/configuration/availability'),
    ),
};
