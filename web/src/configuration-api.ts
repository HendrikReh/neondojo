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
export interface ConfigurationIssue {
  path: string;
  code: string;
  message: string;
}
export class ConfigurationApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
    readonly code = '',
    readonly issues: ConfigurationIssue[] = [],
  ) {
    super(message);
    this.name = 'ConfigurationApiError';
  }
}
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
    throw new ConfigurationApiError(
      200,
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
    throw new ConfigurationApiError(200, 'Invalid profile response');
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
    throw new ConfigurationApiError(200, 'Invalid configuration response');
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
    throw new ConfigurationApiError(200, 'Invalid configuration differences');
  return {
    ...value,
    active_profile: profile,
    current_configuration: current,
  } as unknown as ConfigurationState;
}
export async function configurationRequest<T = unknown>(
  path: string,
  method = 'GET',
  body?: unknown,
): Promise<T> {
  const response = await fetch(`/api${path}`, {
    method,
    ...(body === undefined
      ? {}
      : {
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        }),
  });
  let value: unknown;
  try {
    value = await response.json();
  } catch {
    throw new ConfigurationApiError(
      response.status,
      response.statusText || 'Invalid server response',
    );
  }
  if (!response.ok) {
    const error = object(value) ? value : {};
    throw new ConfigurationApiError(
      response.status,
      String(error.error || response.statusText),
      typeof error.code === 'string' ? error.code : '',
      Array.isArray(error.issues) ? (error.issues as ConfigurationIssue[]) : [],
    );
  }
  return value as T;
}
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
        import('./settings-api'),
        import('./source-analysis-api'),
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
    const value = await configurationRequest<unknown>('/profiles');
    const items = Array.isArray(value)
      ? value
      : object(value) && Array.isArray(value.profiles)
        ? value.profiles
        : null;
    if (!items) throw new ConfigurationApiError(200, 'Invalid profile list');
    return Promise.all(
      items.map(async (item) => {
        if (object(item) && item.configuration) return parseProfile(item);
        if (!object(item) || typeof item.id !== 'string')
          throw new ConfigurationApiError(200, 'Invalid profile list');
        return parseProfile(
          await configurationRequest(
            `/profiles/${encodeURIComponent(item.id)}`,
          ),
        );
      }),
    );
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
