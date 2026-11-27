import { expect, type APIRequestContext, type Page } from '@playwright/test';
import type { components } from '../../src/generated-api';
import {
  gotoAnalysisBoard,
  openWorkspace,
  seedStorage,
  type SeedStorageOptions,
} from './navigation';

type Schemas = components['schemas'];
type RequestCarrier = Page | APIRequestContext;

function apiRequest(target: RequestCarrier): APIRequestContext {
  return 'request' in target ? target.request : target;
}

export const DEFAULT_DATABASE_ID = '00000000-0000-0000-0000-000000000001';
export const DEFAULT_MATCH_ID = '33333333-3333-4333-8333-333333333333';

/** Builds an OpenAPI-typed `Database` mock record. */
export function databaseFixture(
  overrides: Partial<Schemas['Database']> = {},
): Schemas['Database'] {
  const created_at = overrides.created_at ?? '2026-11-10T12:00:00Z';
  return {
    id: DEFAULT_DATABASE_ID,
    name: 'Test database',
    description: '',
    is_default: true,
    is_system: false,
    version: 0,
    match_count: 1,
    created_at,
    updated_at: overrides.updated_at ?? created_at,
    ...overrides,
  };
}

/** Builds an OpenAPI-typed `DatabaseMatch` mock record. */
export function databaseMatchFixture(
  overrides: Partial<Schemas['DatabaseMatch']> = {},
): Schemas['DatabaseMatch'] {
  const filename = overrides.filename ?? 'fixture.sgf';
  return {
    id: DEFAULT_MATCH_ID,
    match_no: 1,
    database_id: DEFAULT_DATABASE_ID,
    filename,
    display_name: overrides.display_name ?? filename,
    format: 'sgf',
    white: 'Alice',
    black: 'Bob',
    match_length: 7,
    position_count: 1,
    game_count: 1,
    created_at: '2026-11-10T12:00:00Z',
    played_on: null,
    tournament: null,
    round: null,
    place: null,
    result_state: 'completed',
    white_score: 7,
    black_score: 3,
    white_pr: null,
    black_pr: null,
    pr_method: null,
    pr_profile: null,
    analysis_status: null,
    source_available: true,
    import_warning_count: 0,
    play_assisted: false,
    play_paused: false,
    play_session_id: null,
    play_phase: null,
    tags: [],
    comment: '',
    version: 0,
    ...overrides,
  };
}

export interface SgfGameOptions {
  white?: string;
  black?: string;
  matchLength?: number;
  whiteScore?: number;
  blackScore?: number;
  moves?: string;
  extraHeaders?: string;
  games?: number;
}

/** Builds a valid SGF backgammon match string (optionally repeated across multiple games). */
export function buildSgfMatch(options: SgfGameOptions = {}): string {
  const suffix = crypto.randomUUID();
  const white = options.white ?? `Player ${suffix}`;
  const black = options.black ?? 'Opponent';
  const matchLength = options.matchLength ?? 7;
  const ws = options.whiteScore ?? 0;
  const bs = options.blackScore ?? 0;
  const moves = options.moves ?? ';W[31hefe];B[42qusu]';
  const extra = options.extraHeaders ?? '';
  const games = options.games ?? 1;
  const game = `(;GM[6]PW[${white}]PB[${black}]${extra}MI[length:${matchLength}][ws:${ws}][bs:${bs}]${moves})`;
  return games <= 1 ? game : Array.from({ length: games }, () => game).join('');
}

export async function createDatabase(
  target: RequestCarrier,
  name: string,
  description?: string,
): Promise<Schemas['Database']> {
  const response = await apiRequest(target).post('/api/databases', {
    data: description === undefined ? { name } : { name, description },
  });
  expect(response.ok(), await response.text()).toBe(true);
  return response.json();
}

export interface ImportSgfOptions {
  sgf?: string;
  game?: SgfGameOptions;
  filename?: string;
  databaseId?: string;
}

export async function importSgfMatch(
  target: RequestCarrier,
  options: ImportSgfOptions = {},
): Promise<Schemas['ImportResponse']> {
  const sgf = options.sgf ?? buildSgfMatch(options.game);
  const filename = options.filename ?? `fixture-${crypto.randomUUID()}.sgf`;
  const url = options.databaseId
    ? `/api/imports?database_id=${options.databaseId}`
    : '/api/imports';
  const response = await apiRequest(target).post(url, {
    multipart: {
      file: {
        name: filename,
        mimeType: 'application/octet-stream',
        buffer: Buffer.from(sgf),
      },
    },
  });
  expect(response.status(), await response.text()).toBe(201);
  return response.json();
}

export async function listMatchPositions(
  target: RequestCarrier,
  matchId: string,
  limit = 100,
): Promise<Schemas['Position'][]> {
  const response = await apiRequest(target).get(
    `/api/positions?match_id=${matchId}&limit=${limit}`,
  );
  expect(response.ok(), await response.text()).toBe(true);
  const page: Schemas['PositionPage'] = await response.json();
  return page.items;
}

export async function bookmarkPositions(
  target: RequestCarrier,
  positionIds: string[],
): Promise<void> {
  const request = apiRequest(target);
  for (const id of positionIds) {
    const response = await request.put(`/api/positions/${id}/bookmark`);
    expect(response.ok(), await response.text()).toBe(true);
  }
}

export async function annotateMatch(
  target: RequestCarrier,
  matchId: string,
  data: { expected_version?: number; tags: string[]; comment: string },
) {
  const response = await apiRequest(target).put(
    `/api/matches/${matchId}/annotation`,
    {
      data: {
        expected_version: data.expected_version ?? 0,
        tags: data.tags,
        comment: data.comment,
      },
    },
  );
  expect(response.ok(), await response.text()).toBe(true);
  return response.json();
}

export async function annotatePosition(
  target: RequestCarrier,
  positionId: string,
  data: { expected_version?: number; tags: string[]; comment: string },
) {
  const response = await apiRequest(target).put(
    `/api/positions/${positionId}/annotation`,
    {
      data: {
        expected_version: data.expected_version ?? 0,
        tags: data.tags,
        comment: data.comment,
      },
    },
  );
  expect(response.ok(), await response.text()).toBe(true);
  return response.json();
}

export async function waitForJobCompletion(
  target: RequestCarrier,
  jobId: string,
  timeout?: number,
): Promise<Schemas['Job']> {
  const request = apiRequest(target);
  await expect
    .poll(
      async () => {
        const job = (await (
          await request.get(`/api/jobs/${jobId}`)
        ).json()) as Schemas['Job'];
        return job.error ?? job.status;
      },
      timeout ? { timeout } : undefined,
    )
    .toBe('completed');
  return (await request.get(`/api/jobs/${jobId}`)).json();
}

export interface SeedSgfMatchOptions {
  /** Create a fresh isolated database with this name. */
  databaseName?: string;
  databaseDescription?: string;
  /** Import into an existing database instead of creating a new one. */
  databaseId?: string;
  sgf?: string;
  game?: SgfGameOptions;
  filename?: string;
  /** Bookmark all positions (`'all'`), first position (`'first'`), or none (`false`). */
  bookmark?: 'all' | 'first' | false;
  /** Limit when fetching positions (`0` skips fetching). Defaults to `100`. */
  positionsLimit?: number;
  /** Seed `localStorage` via `seedStorage` (`matchId` / `databaseId` auto-populated when `open` is set). */
  storage?: SeedStorageOptions;
  /** Navigate to `/` and optionally open a workspace or wait for the analysis board. */
  open?: 'analysis' | 'library' | 'positions' | 'root' | false;
}

export async function seedSgfMatch(
  page: Page,
  options: SeedSgfMatchOptions = {},
) {
  const database = options.databaseName
    ? await createDatabase(
        page,
        options.databaseName,
        options.databaseDescription,
      )
    : undefined;
  const databaseId = database?.id ?? options.databaseId;
  const imported = await importSgfMatch(page, {
    sgf: options.sgf,
    game: options.game,
    filename: options.filename,
    databaseId,
  });
  const limit = options.positionsLimit ?? 100;
  const positions =
    limit > 0 ? await listMatchPositions(page, imported.match.id, limit) : [];
  if (options.bookmark === 'all') {
    await bookmarkPositions(
      page,
      positions.map((p) => p.id),
    );
  } else if (options.bookmark === 'first' && positions[0]) {
    await bookmarkPositions(page, [positions[0].id]);
  }
  if (options.storage || options.open) {
    const storage: SeedStorageOptions = { ...options.storage };
    if (databaseId && storage.databaseId === undefined)
      storage.databaseId = databaseId;
    if (options.open === 'analysis' && storage.workspace === undefined) {
      storage.workspace = { matchId: imported.match.id, gameNo: 1 };
    }
    await seedStorage(page, storage);
  }
  if (options.open === 'analysis') {
    await gotoAnalysisBoard(page);
  } else if (options.open === 'library') {
    await page.goto('/');
    await openWorkspace(page, 'library');
  } else if (options.open === 'positions') {
    await page.goto('/');
    await openWorkspace(page, 'positions');
  } else if (options.open === 'root') {
    await page.goto('/');
  }
  return { database, match: imported.match, imported, positions };
}
