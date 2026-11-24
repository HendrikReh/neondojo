import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  configurationRequest,
  parseConfigurationState,
  parseProfile,
  parseAvailability,
} from './configuration-api';
import { HttpError } from '../../shared/http/api';
import { defaultConfiguration } from './configuration-model';
afterEach(() => vi.unstubAllGlobals());
describe('configuration API contract', () => {
  it('rejects malformed availability reports before they reach rendering', () => {
    expect(() =>
      parseAvailability({ revision: 1, valid: false, issues: null }),
    ).toThrow();
    expect(() =>
      parseAvailability({
        revision: 1,
        valid: true,
        issues: [
          { path: 'analysis.engine', code: 'unavailable', message: 'Missing' },
        ],
      }),
    ).toThrow();
    expect(parseAvailability({ revision: 2, valid: true, issues: [] })).toEqual(
      { revision: 2, valid: true, issues: [] },
    );
  });
  it('validates complete envelopes and semantic configuration', () => {
    const profile = {
      id: '00000000-0000-4000-8000-000000000001',
      name: 'Standard',
      revision: 1,
      configuration: structuredClone(defaultConfiguration),
    };
    const state = {
      revision: 1,
      active_profile: profile,
      current_configuration: profile.configuration,
      dirty: false,
      differences: [],
    };
    expect(parseConfigurationState(state).active_profile.name).toBe('Standard');
    expect(() => parseConfigurationState({ ...state, revision: 0 })).toThrow();
    expect(() => parseProfile({ ...profile, configuration: {} })).toThrow();
    expect(() =>
      parseConfigurationState({
        ...state,
        current_configuration: { ...profile.configuration, schema_version: 3 },
      }),
    ).toThrow();
  });
  it('retains status, code and field issues on a rejected mutation', async () => {
    vi.stubGlobal(
      'fetch',
      async () =>
        new Response(
          JSON.stringify({
            error: 'Book unavailable',
            code: 'unavailable_resources',
            issues: [
              {
                path: 'opening_book.path',
                code: 'missing_resource',
                message: 'Missing book',
              },
            ],
          }),
          { status: 422 },
        ),
    );
    try {
      await configurationRequest('/profiles/test/activate', 'POST', {});
      throw new Error('Expected rejection');
    } catch (error) {
      expect(error).toBeInstanceOf(HttpError);
      expect((error as HttpError).status).toBe(422);
      expect((error as HttpError).issues[0].path).toBe('opening_book.path');
    }
  });
  it('rejects non-JSON responses without reporting a successful write', async () => {
    vi.stubGlobal(
      'fetch',
      async () => new Response('bad response', { status: 200 }),
    );
    await expect(configurationRequest('/configuration')).rejects.toThrow(
      'error.invalidResponse',
    );
  });
});
