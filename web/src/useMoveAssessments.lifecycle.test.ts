import { beforeEach, expect, it, vi } from 'vitest';
import type {
  GameMoveAssessments,
  MoveAssessmentScope,
} from './useMoveAssessments';

// A minimal hook scheduler lets this Node test control the interval between
// rendering a new selection and React cleaning up its preceding effect.
const hooks = vi.hoisted(() => ({
  state: undefined as unknown,
  ref: undefined as { current: string } | undefined,
  deps: undefined as unknown[] | undefined,
  effect: undefined as (() => () => void) | undefined,
  cleanup: undefined as (() => void) | undefined,
  request: vi.fn(),
}));
vi.mock('react', () => ({
  useRef(value: string) {
    return (hooks.ref ??= { current: value });
  },
  useState(initial: unknown) {
    if (hooks.state === undefined) hooks.state = initial;
    return [
      hooks.state,
      (next: unknown) => {
        hooks.state = next;
      },
    ];
  },
  useEffect(effect: () => () => void, deps: unknown[]) {
    if (!hooks.deps || deps.some((dep, i) => dep !== hooks.deps?.[i])) {
      hooks.deps = deps;
      hooks.effect = effect;
    }
  },
}));
vi.mock('./api', () => ({ request: hooks.request }));
import { useMoveAssessments } from './useMoveAssessments';

function commit() {
  if (!hooks.effect) return;
  hooks.cleanup?.();
  hooks.cleanup = hooks.effect();
  hooks.effect = undefined;
}
function deferred() {
  let resolve!: (data: GameMoveAssessments) => void;
  const promise = new Promise<GameMoveAssessments>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
async function settle() {
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  hooks.cleanup?.();
  hooks.state =
    hooks.ref =
    hooks.deps =
    hooks.effect =
    hooks.cleanup =
      undefined;
  hooks.request.mockReset();
});

it('refetches and fences an earlier response when an individual analysis finishes', async () => {
  const old = deferred();
  const refreshed = deferred();
  hooks.request
    .mockReturnValueOnce(old.promise)
    .mockReturnValueOnce(refreshed.promise);
  const scope: MoveAssessmentScope = { matchId: 'match', gameNo: 2 };
  useMoveAssessments(scope);
  commit();
  const firstSignal = hooks.request.mock.calls[0][1].signal as AbortSignal;
  const completedHint = {
    ...scope,
    analysisRevision: 'hint:completed:2026-11-09T00:35:00Z',
  };
  const response: GameMoveAssessments = {
    match_id: 'match',
    game_no: 2,
    job_id: null,
    items: [
      {
        position_id: 'position',
        severity: 'bad',
        equity_loss: 0.08,
        source: 'native',
        engine: 'GNUbg hint',
        depth: '2-ply',
        depth_plies: 2,
        job_id: 'hint',
      },
    ],
  };
  useMoveAssessments(completedHint);
  // The old request can resolve before cleanup aborts it. Render identity alone
  // must reject it so previous assessments never flash after hint completion.
  old.resolve(response);
  await settle();
  expect(useMoveAssessments(completedHint).assessments).toEqual({});
  commit();
  expect(firstSignal.aborted).toBe(true);
  expect(hooks.request).toHaveBeenCalledTimes(2);
  expect(hooks.request.mock.calls[1][0]).toBe(hooks.request.mock.calls[0][0]);
  refreshed.resolve(response);
  await settle();
  expect(useMoveAssessments(completedHint)).toEqual({
    assessments: { position: response.items[0] },
    loading: false,
    error: null,
  });
});
