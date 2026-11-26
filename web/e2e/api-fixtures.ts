import type { Page, Route } from '@playwright/test';
import type { components } from '../src/generated-api';
import {
  defaultConfiguration,
  type ProfileConfiguration,
} from '../src/settings/configuration/configuration-model';

type Schemas = components['schemas'];

export function fulfillJson<Name extends keyof Schemas>(
  route: Route,
  payload: Schemas[Name],
  status = 200,
) {
  return route.fulfill({ status, json: payload });
}

export function fulfillJsonList<Name extends keyof Schemas>(
  route: Route,
  payload: Schemas[Name][],
) {
  return route.fulfill({ json: payload });
}

// GET serializes the host overrides even when they are unset. The same API
// schema also describes PUT, where omitting an override preserves its value.
export function settingsFixture(
  c: ProfileConfiguration = defaultConfiguration,
): Required<Schemas['AppSettings']> {
  return {
    version: 1,
    player_name: c.general.player_name,
    analysis_engine: c.analysis.engine,
    analysis_plies: c.analysis.plies,
    play_engine: c.play.engine,
    play_plies: c.play.plies,
    play_top_two: c.play.top_two ?? false,
    gnubg_path: '/usr/bin/gnubg',
    gnubg_threads: null,
    gnubg_play_threads: null,
    open_sage_threads: null,
    open_sage_play_threads: null,
    parallel_analysis_jobs: null,
    bearoff_direction: c.appearance.bearoff_direction,
    default_play_mode: c.play.default_mode,
    tutor_threshold: c.play.tutor_threshold,
    training_auto_analysis: c.play.training_auto_analysis,
  };
}

type SettingsMock = {
  request: { method(): string; postDataJSON(): Schemas['AppSettings'] };
  respond(settings: Required<Schemas['AppSettings']>): Promise<void>;
  fail(status: number, body: Schemas['ErrorBody']): Promise<void>;
};

export function mockSettings(
  page: Page,
  handle: (mock: SettingsMock) => Promise<void>,
) {
  return page.route('**/api/settings', (route) =>
    handle({
      request: route.request(),
      respond: (settings) => fulfillJson<'AppSettings'>(route, settings),
      fail: (status, body) => fulfillJson<'ErrorBody'>(route, body, status),
    }),
  );
}

type EngineCatalog = Schemas['EngineInfo'][];
const defaultCatalog: EngineCatalog = [
  {
    id: 'gnubg',
    name: 'GNU Backgammon',
    available: true,
    version: 'GNUbg 1.08',
    plies: [1, 2, 3, 4, 5],
    max_parallel_threads: 7,
  },
  {
    id: 'bgsage',
    name: 'NeonGammon',
    available: false,
    version: null,
    plies: [2, 3, 4],
    difficulty_presets: false,
    max_parallel_threads: 9,
  },
];

export function mockEngineCatalog(
  page: Page,
  read: () => EngineCatalog | Promise<EngineCatalog> = () => defaultCatalog,
) {
  return page.route(
    (url) => url.pathname === '/api/engines',
    async (route) =>
      route.request().method() === 'GET'
        ? fulfillJsonList<'EngineInfo'>(route, await read())
        : route.fallback(),
  );
}
