import type { Page, Route } from '@playwright/test';
import type { components } from '../../src/generated-api';
import {
  defaultConfiguration,
  type ProfileConfiguration,
} from '../../src/settings/configuration/configuration-model';

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
  status = 200,
) {
  return route.fulfill({ status, json: payload });
}

export function fulfillNullableJson<Name extends keyof Schemas>(
  route: Route,
  payload: Schemas[Name] | null,
  status = 200,
) {
  return route.fulfill({ status, json: payload });
}

/** Explicit opt-in only for tests that verify resilience to malformed wire payloads. */
export function fulfillMalformedJson(
  route: Route,
  payload: unknown,
  status = 200,
) {
  return route.fulfill({ status, json: payload });
}

export function fulfillError(
  route: Route,
  error: string | Schemas['ErrorBody'],
  status: number,
) {
  return fulfillJson<'ErrorBody'>(
    route,
    typeof error === 'string' ? { error } : error,
    status,
  );
}

export function fulfillPlaySession(
  route: Route,
  session: Schemas['PlaySession'],
  status = 200,
) {
  return fulfillJson<'PlaySession'>(route, session, status);
}

export function fulfillDecks(
  route: Route,
  decks: Schemas['Deck'][],
  status = 200,
) {
  return fulfillJsonList<'Deck'>(route, decks, status);
}

export function fulfillMistakes(
  route: Route,
  payload: Schemas['Mistakes'],
  status = 200,
) {
  return fulfillJson<'Mistakes'>(route, payload, status);
}

export function fulfillMistakePractice(
  route: Route,
  payload: Schemas['MistakePractice'],
  status = 200,
) {
  return fulfillJson<'MistakePractice'>(route, payload, status);
}

export function fulfillMistakeAttempt(
  route: Route,
  payload: Schemas['MistakeAttempt'],
  status = 200,
) {
  return fulfillJson<'MistakeAttempt'>(route, payload, status);
}

export function fulfillMistakeAnswer(
  route: Route,
  payload: Schemas['MistakeAnswer'],
  status = 200,
) {
  return fulfillJson<'MistakeAnswer'>(route, payload, status);
}

export function fulfillStudyReportPage(
  route: Route,
  payload: Schemas['StudyReportPage'],
  status = 200,
) {
  return fulfillJson<'StudyReportPage'>(route, payload, status);
}

export function fulfillAnalysisEstimate(
  route: Route,
  payload: Schemas['AnalysisTimeEstimate'],
  status = 200,
) {
  return fulfillJson<'AnalysisTimeEstimate'>(route, payload, status);
}

export function fulfillAnalysisRemainingTime(
  route: Route,
  payload: Schemas['AnalysisRemainingTime'],
  status = 200,
) {
  return fulfillJson<'AnalysisRemainingTime'>(route, payload, status);
}

export function fulfillPositionAnalysisResult(
  route: Route,
  payload: Schemas['SelectedPositionAnalysis'] | null,
  status = 200,
) {
  return fulfillNullableJson<'SelectedPositionAnalysis'>(
    route,
    payload,
    status,
  );
}

export function fulfillBenchmarkHistory(
  route: Route,
  payload: Schemas['BenchmarkHistory'],
  status = 200,
) {
  return fulfillJson<'BenchmarkHistory'>(route, payload, status);
}

export function fulfillBenchmarkStatus(
  route: Route,
  payload: Schemas['BenchmarkStatus'],
  status = 200,
) {
  return fulfillJson<'BenchmarkStatus'>(route, payload, status);
}

export function fulfillBenchmarkReference(
  route: Route,
  payload: Schemas['BenchmarkReference'],
  status = 200,
) {
  return fulfillJson<'BenchmarkReference'>(route, payload, status);
}

export function fulfillBenchmarkRun(
  route: Route,
  payload: Schemas['BenchmarkRun'],
  status = 200,
) {
  return fulfillJson<'BenchmarkRun'>(route, payload, status);
}

export function fulfillBenchmarkRuns(
  route: Route,
  payload: Schemas['BenchmarkRun'][],
  status = 200,
) {
  return fulfillJsonList<'BenchmarkRun'>(route, payload, status);
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
