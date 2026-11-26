import {
  fulfillJson,
  fulfillJsonList,
  mockSettings,
  settingsFixture,
} from './api-fixtures';
import type { Page, Route } from '@playwright/test';
import {
  defaultConfiguration,
  configurationDifferences,
  type ProfileConfiguration,
  type ConfigurationDifference,
} from '../src/settings/configuration/configuration-model';
import type { ConfigurationProfile } from '../src/settings/configuration/configuration-api';

export const secondProfileId = '00000000-0000-4000-8000-000000000002';
export interface ProfileBackendOptions {
  initialized?: boolean;
  legacyEndpoints?: boolean;
  configuration?: ProfileConfiguration;
  profiles?: ConfigurationProfile[];
}
export async function profileBackend(
  page: Page,
  options: ProfileBackendOptions = {},
) {
  let initialized = options.initialized ?? true;
  const first = structuredClone(options.configuration ?? defaultConfiguration);
  first.opening_book.path ||= '/books/default.db';
  first.opening_book.xg_path ||= '/books/XG.ob';
  const other = structuredClone(first);
  other.appearance.mode = 'dark';
  other.analysis.plies = 4;
  const now = '2026-11-10T12:00:00Z';
  const initialProfiles: ConfigurationProfile[] = options.profiles ?? [
    {
      id: '00000000-0000-4000-8000-000000000001',
      name: 'Standard',
      revision: 1,
      configuration: first,
    },
    {
      id: secondProfileId,
      name: 'Deep analysis',
      revision: 1,
      configuration: other,
    },
  ];
  const profiles = initialProfiles.map((profile) => ({
    ...profile,
    created_at: profile.created_at ?? now,
    updated_at: profile.updated_at ?? now,
  }));
  const differences: ConfigurationDifference[] = [];
  const control = {
    state: {
      revision: 1,
      active_profile: profiles[0],
      current_configuration: first,
      dirty: false,
      differences,
    },
    profiles: structuredClone(profiles),
    calls: [] as {
      method: string;
      path: string;
      body: Record<string, unknown> | null;
    }[],
    failure: null as {
      status: number;
      code: string;
      message: string;
      path?: string;
    } | null,
    failNext(
      status: number,
      code = 'test_failure',
      message = 'Request failed',
      path?: string,
    ) {
      this.failure = { status, code, message, path };
    },
    updateRemote(configuration: ProfileConfiguration) {
      this.state = {
        ...this.state,
        revision: this.state.revision + 1,
        current_configuration: structuredClone(configuration),
      };
      sync();
    },
  };
  function sync() {
    control.state.active_profile = structuredClone(
      control.profiles.find((p) => p.id === control.state.active_profile.id)!,
    );
    control.state.differences = configurationDifferences(
      control.state.active_profile.configuration,
      control.state.current_configuration,
    );
    control.state.dirty = control.state.differences.length > 0;
  }
  async function error(
    route: Route,
    status: number,
    code: string,
    message = code,
  ) {
    return fulfillJson<'ErrorBody'>(
      route,
      { error: message, code, issues: [] },
      status,
    );
  }
  await page.route(
    /\/api\/(?:configuration|profiles)(?:[/?].*)?$/,
    async (route) => {
      const request = route.request(),
        method = request.method(),
        path = new URL(request.url()).pathname.replace('/api', '');
      const body =
        method === 'GET'
          ? null
          : (request.postDataJSON() as Record<string, unknown>);
      control.calls.push({ method, path, body });
      if (
        control.failure &&
        (!control.failure.path || path.includes(control.failure.path))
      ) {
        const f = control.failure;
        control.failure = null;
        return error(route, f.status, f.code, f.message);
      }
      if (path === '/configuration/initialize' && method === 'POST') {
        if (!initialized) {
          initialized = true;
          const config = structuredClone(
            body!.configuration as ProfileConfiguration,
          );
          const profile = { ...control.profiles[0], configuration: config };
          control.profiles = [profile];
          control.state = {
            ...control.state,
            active_profile: profile,
            current_configuration: config,
          };
          sync();
        }
        return fulfillJson<'ConfigurationState'>(route, control.state);
      }
      if (!initialized)
        return error(route, 409, 'configuration_not_initialized');
      if (method === 'GET') {
        if (path === '/configuration/availability')
          return fulfillJson<'ConfigurationAvailability'>(route, {
            revision: control.state.revision,
            valid: true,
            issues: [],
          });
        if (path === '/configuration')
          return fulfillJson<'ConfigurationState'>(route, control.state);
        if (path === '/profiles')
          return fulfillJsonList<'ConfigurationProfile'>(
            route,
            control.profiles,
          );
        const profile = control.profiles.find(
          (p) => `/profiles/${p.id}` === path,
        );
        return profile
          ? fulfillJson<'ConfigurationProfile'>(route, profile)
          : error(route, 404, 'not_found');
      }
      const expected = body?.expected_state_revision ?? body?.expected_revision;
      if (expected !== undefined && expected !== control.state.revision)
        return error(route, 409, 'revision_conflict');
      const id = path.split('/')[2];
      const profile = control.profiles.find((p) => p.id === id);
      if (path.startsWith('/profiles/') && !profile)
        return error(route, 404, 'not_found');
      if (
        profile &&
        body?.expected_profile_revision !== undefined &&
        body.expected_profile_revision !== profile.revision
      )
        return error(route, 409, 'revision_conflict');
      const name =
        typeof body?.name === 'string'
          ? body.name.normalize('NFC').trim()
          : undefined;
      if (
        name !== undefined &&
        (!name || [...name].length > 80 || /\p{Cc}/u.test(name))
      )
        return error(route, 422, 'invalid_name');
      if (
        name !== undefined &&
        control.profiles.some(
          (p) =>
            p.id !== (method === 'PATCH' ? id : undefined) &&
            p.name.toLocaleLowerCase() === name.toLocaleLowerCase(),
        )
      )
        return error(route, 409, 'duplicate_name');
      if (path === '/configuration' && method === 'PUT')
        control.state.current_configuration = structuredClone(
          body!.configuration as ProfileConfiguration,
        );
      else if (path === '/profiles' && method === 'POST') {
        const created = {
          id: `00000000-0000-4000-8000-${String(control.profiles.length + 10).padStart(12, '0')}`,
          name: name!,
          revision: 1,
          configuration: structuredClone(
            body!.configuration as ProfileConfiguration,
          ),
          created_at: now,
          updated_at: now,
        };
        control.profiles.push(created);
        control.state.active_profile = created;
        control.state.current_configuration = structuredClone(
          created.configuration,
        );
      } else if (method === 'PATCH' && profile) {
        profile.name = name!;
        profile.revision++;
      } else if (method === 'DELETE' && profile) {
        if (profile.id === control.state.active_profile.id)
          return error(route, 409, 'active_profile');
        control.profiles = control.profiles.filter((p) => p.id !== id);
      } else if (path.endsWith('/snapshot') && method === 'PUT' && profile) {
        profile.configuration = structuredClone(
          body!.configuration as ProfileConfiguration,
        );
        profile.revision++;
        control.state.current_configuration = structuredClone(
          profile.configuration,
        );
      } else if (path.endsWith('/activate') && method === 'POST' && profile) {
        if (
          body?.expected_target_revision !== profile.revision ||
          body.expected_source_revision !==
            control.state.active_profile.revision
        )
          return error(route, 409, 'revision_conflict');
        if (body?.source_action === 'save_current') {
          const source = control.profiles.find(
            (p) => p.id === control.state.active_profile.id,
          )!;
          source.configuration = structuredClone(
            body.current_configuration as ProfileConfiguration,
          );
          source.revision++;
        }
        control.state.active_profile = profile;
        control.state.current_configuration = structuredClone(
          profile.configuration,
        );
      } else if (path.endsWith('/validate'))
        return fulfillJson<'ConfigurationValidation'>(route, {
          valid: true,
          issues: [],
        });
      else if (path === '/configuration/restore-profile') {
        control.state.current_configuration = structuredClone(
          control.state.active_profile.configuration,
        );
      } else return error(route, 404, 'not_found');
      control.state.revision++;
      sync();
      return fulfillJson<'ConfigurationState'>(route, control.state);
    },
  );
  if (options.legacyEndpoints !== false) {
    await mockSettings(page, ({ request, respond, fail }) => {
      if (request.method() !== 'GET')
        return fail(422, {
          error: 'unexpected_legacy_write',
          code: 'unexpected_legacy_write',
          issues: [],
        });
      return respond(settingsFixture(control.state.current_configuration));
    });
    await page.route('**/api/opening-book', async (route) => {
      if (route.request().method() !== 'GET')
        return error(route, 422, 'unexpected_legacy_write');
      return fulfillJson<'BookSettings'>(route, {
        config: {
          version: 1,
          ...control.state.current_configuration.opening_book,
        },
        status: {
          available: true,
          path: control.state.current_configuration.opening_book.path,
          entries: 0,
          contexts: [],
          error: null,
        },
        xg_status: {
          available: true,
          path: control.state.current_configuration.opening_book.xg_path,
          entries: 0,
          source_url: 'https://www.extremegammon.com/GetOb.aspx',
          license: 'Public domain',
          notes: '',
        },
      });
    });
  }
  return control;
}
