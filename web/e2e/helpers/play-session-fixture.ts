import { expect, type Page, type Route } from '@playwright/test';
import type { components } from '../../src/generated-api';
import { nativeSteps } from '../../src/shared/board/play-sound-events';
import type {
  PlayAlternative,
  PlayDecision,
  PlaySession,
} from '../../src/workspaces/play/play-api';
import { fulfillJson, fulfillJsonList } from './api-fixtures';
import { openWorkspace, seedStorage } from './navigation';
import { eitherOrder } from './play-turn-fixtures';

type Schemas = components['schemas'];

export const INITIAL_BOARD: readonly number[] = [
  0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0, 0, 2,
  0,
];

export const DEFAULT_PLAY_SESSION_ID = '9ef7a4aa-56c7-4b32-b7dd-78029632e879';
export const DEFAULT_PLAY_DECISION_ID = '07aab353-8dc9-40c8-91a9-3377483240b5';

export function initialBoard(): number[] {
  return [...INITIAL_BOARD];
}

export function playPositionFixture(
  overrides: Partial<Schemas['Position']> = {},
  sessionId = DEFAULT_PLAY_SESSION_ID,
): Schemas['Position'] {
  return {
    id: sessionId,
    match_uuid: sessionId,
    ordinal: 0,
    game_no: 1,
    ply: 0,
    player: 0,
    dice: [3, 1],
    board: initialBoard(),
    position_id: '4HPwATDgc/ABMA',
    gnubg_match_id: 'cInlAAAAAAAE',
    notation: '',
    ...overrides,
  };
}

export type PlaySessionOverrides = Omit<Partial<PlaySession>, 'position'> & {
  position?: Partial<Schemas['Position']>;
  /** Populate `legal_moves` and `legal_turns` from dice + notation when not explicitly provided. */
  defaultMove?: { dice: number[]; notation: string } | false;
};

/**
 * Builds a complete, OpenAPI-typed `PlaySession` fixture with sensible defaults.
 */
export function playSessionFixture(
  overrides: PlaySessionOverrides = {},
): PlaySession {
  const { position: posOverrides, defaultMove, ...rest } = overrides;
  const id = rest.id ?? DEFAULT_PLAY_SESSION_ID;
  const human = rest.human_player ?? 0;
  const phase = rest.phase ?? 'move';
  const defaultDice = phase === 'move' ? [3, 1] : [];
  const position = playPositionFixture(
    {
      id,
      match_uuid: id,
      player: human,
      dice: defaultDice,
      ...posOverrides,
    },
    id,
  );
  const moveSpec =
    defaultMove === undefined
      ? phase === 'move' &&
        rest.legal_moves === undefined &&
        rest.legal_turns === undefined
        ? {
            dice: position.dice.length ? position.dice : [3, 1],
            notation: '8/5 6/5',
          }
        : false
      : defaultMove;

  const legal_moves =
    rest.legal_moves ??
    (moveSpec
      ? [
          {
            notation: moveSpec.notation,
            steps: nativeSteps(moveSpec.notation, human),
          },
        ]
      : []);
  const legal_turns =
    rest.legal_turns !== undefined
      ? rest.legal_turns
      : moveSpec
        ? eitherOrder(moveSpec.dice, moveSpec.notation, human)
        : undefined;

  return {
    id,
    version: 0,
    human_player: human,
    human_name: 'Player',
    match_length: 7,
    bearoff_direction: 'right',
    engine: { id: 'gnubg', version: 'GNU Backgammon 1.08.003', plies: 3 },
    position,
    phase,
    can_double: false,
    ...(legal_turns !== undefined ? { legal_turns } : {}),
    legal_moves,
    history: [],
    scores: [0, 0],
    game_no: 1,
    winner: null,
    opening: null,
    ...rest,
  };
}

export function playReviewFixture(
  overrides: Partial<Schemas['PlayReview']> = {},
): Schemas['PlayReview'] {
  return {
    analysis_plies: 3,
    assisted: false,
    assisted_first_attempt_count: 0,
    final_choice_loss: 0,
    first_attempt_count: 0,
    first_attempt_loss: 0,
    first_attempt_rating: null,
    first_attempt_rating_decisions: 0,
    first_attempt_rating_method:
      'gnubg-original-choices-emg500-reached-positions-v1',
    first_attempt_rating_profile: 'fixture',
    games: [],
    independent_first_attempt_count: 0,
    retry_count: 0,
    ...overrides,
  };
}

export function playDecisionFixture(
  overrides: Partial<PlayDecision> = {},
): PlayDecision {
  return {
    id: DEFAULT_PLAY_DECISION_ID,
    action: 'move',
    move_index: 0,
    original_action: 'move',
    original_move_index: 0,
    warning: false,
    revealed: true,
    assisted: false,
    retry_count: 0,
    first_loss: 0,
    loss: 0,
    first_severity: 'none',
    severity: 'none',
    alternatives: [] as PlayAlternative[],
    attempts: [],
    committed: false,
    ...overrides,
  };
}

export interface PlaySessionMockOptions {
  /** Initial session state. */
  session: PlaySession;
  /** Register the `/api/play/sessions` collection route (default `true`). */
  mockCollection?: boolean;
  /** Register the `/api/play/sessions/{id}/actions` route (default `true`). */
  mockActions?: boolean;
  /** Custom handler for `POST /api/play/sessions`. */
  onCreate?: (
    body: Record<string, unknown>,
    current: PlaySession,
    route: Route,
  ) => PlaySession | void | Promise<PlaySession | void>;
  /**
   * Custom handler for `POST /api/play/sessions/{id}/actions`.
   * Return a `PlaySession` to update state and fulfill 200, `null` to fulfill 503,
   * or handle `route` directly and return `undefined`.
   */
  onAction?: (
    body: Record<string, unknown>,
    current: PlaySession,
    route: Route,
  ) => PlaySession | null | void | Promise<PlaySession | null | void>;
  /** Custom handler for `POST /api/play/sessions/{id}/feedback`. */
  onFeedback?: (
    body: Record<string, unknown>,
    current: PlaySession,
    route: Route,
  ) => PlaySession | void | Promise<PlaySession | void>;
  /** Seed `neondojo.play-session` before load (default `true`). */
  seedSession?: boolean;
  locale?: 'en' | 'de';
  theme?: string;
  playSounds?: 'on' | 'off';
  /** Navigate to `/` and click Playing tab automatically (default `false`). */
  open?: boolean;
  /** Wait for `.play-board` to be visible when `open: true` (default `false`). */
  waitForBoard?: boolean;
}

/**
 * Installs OpenAPI-typed `/api/play/sessions*` routes and optional localStorage seeding.
 */
export async function mockPlaySession(
  page: Page,
  options: PlaySessionMockOptions,
) {
  let state = options.session;
  const id = state.id;
  const writes: unknown[] = [];
  const feedbackWrites: unknown[] = [];

  if (
    options.seedSession !== false ||
    options.locale ||
    options.theme ||
    options.playSounds
  ) {
    await seedStorage(page, {
      ...(options.seedSession !== false ? { playSessionId: id } : {}),
      locale: options.locale,
      theme: options.theme,
      playSounds: options.playSounds,
    });
  }

  if (options.mockCollection !== false) {
    await page.route('**/api/play/sessions', async (route) => {
      if (route.request().method() === 'POST') {
        if (options.onCreate) {
          const created = await options.onCreate(
            route.request().postDataJSON(),
            state,
            route,
          );
          if (created) {
            state = created;
            return fulfillJson<'PlaySession'>(route, state, 201);
          }
          return;
        }
        return fulfillJson<'PlaySession'>(route, state, 201);
      }
      return fulfillJsonList<'PlaySession'>(route, [state]);
    });
  }

  await page.route(`**/api/play/sessions/${id}`, (route) =>
    fulfillJson<'PlaySession'>(route, state),
  );

  if (options.mockActions !== false) {
    await page.route(`**/api/play/sessions/${id}/actions`, async (route) => {
      const body = route.request().postDataJSON();
      writes.push(body);
      if (options.onAction) {
        const next = await options.onAction(body, state, route);
        if (next === null) {
          return fulfillJson<'ErrorBody'>(
            route,
            { error: 'Action failed' },
            503,
          );
        }
        if (next !== undefined) {
          state = next;
          return fulfillJson<'PlaySession'>(route, state);
        }
        return;
      }
      return fulfillJson<'PlaySession'>(route, state);
    });
  }

  if (options.onFeedback) {
    await page.route(`**/api/play/sessions/${id}/feedback`, async (route) => {
      const body = route.request().postDataJSON();
      feedbackWrites.push(body);
      const next = await options.onFeedback!(body, state, route);
      if (next !== undefined) {
        state = next;
        return fulfillJson<'PlaySession'>(route, state);
      }
    });
  }

  if (options.open) {
    await page.goto('/');
    await openWorkspace(page, 'play');
    if (options.waitForBoard) {
      await expect(page.locator('.play-board')).toBeVisible();
    }
  }

  return {
    id,
    writes,
    feedbackWrites,
    getState: () => state,
    setState: (next: PlaySession) => {
      state = next;
    },
    updateState: (patch: Partial<PlaySession>) => {
      state = { ...state, ...patch };
      return state;
    },
    point: (n: number) =>
      page.locator(
        `[data-play-point="${state.human_player ? 25 - n : n}"] .play-point`,
      ),
  };
}
