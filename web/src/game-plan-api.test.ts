import { afterEach, expect, it, vi } from 'vitest';
import {
  GamePlanCache,
  gamePlanBoardKey,
  loadGamePlans,
  validateGamePlans,
  type GamePlans,
} from './game-plan-api';

const board = () => {
  const result = Array<number>(26).fill(0);
  result[24] = 2;
  result[1] = -2;
  return result;
};
const plans: GamePlans = {
  classifier: 'opensage_rules_v1',
  source_revision: '139427d826dd67798226e7d23c003c90315306ba',
  game_over: false,
  white: 'anchoring',
  black: 'priming',
};
afterEach(() => vi.unstubAllGlobals());

it('posts the exact absolute-color checker placement and forwards cancellation', async () => {
  const fetch = vi
    .fn<typeof globalThis.fetch>()
    .mockResolvedValue(Response.json(plans));
  vi.stubGlobal('fetch', fetch);
  const abort = new AbortController();
  expect(await loadGamePlans(board(), abort.signal)).toEqual(plans);
  expect(fetch).toHaveBeenCalledWith('/api/game-plans', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ board: board() }),
    signal: abort.signal,
  });
});

it.each([
  { ...plans, classifier: 'guessed' },
  { ...plans, source_revision: '' },
  { ...plans, game_over: 'false' },
  { ...plans, white: 'optimal' },
  { ...plans, black: null },
  null,
])('rejects malformed classification responses: %j', async (data) => {
  vi.stubGlobal('fetch', async () => Response.json(data));
  await expect(loadGamePlans(board())).rejects.toMatchObject({
    key: 'error.invalidResponse',
  });
  expect(validateGamePlans(data)).toBeUndefined();
});

it('accepts every supported plan and preserves the terminal-game flag', () => {
  for (const white of [
    'pure_race',
    'racing',
    'attacking',
    'priming',
    'anchoring',
  ])
    expect(
      validateGamePlans({ ...plans, white, game_over: true }),
    ).toMatchObject({ white, game_over: true });
});

it.each(
  [
    [],
    Array<number>(26),
    Array(27).fill(0),
    Array(26).fill(0.5),
    [1, ...Array(25).fill(0)],
    [...Array(25).fill(0), -1],
    [0, 16, ...Array(24).fill(0)],
    [0, 10, 10, ...Array(23).fill(0)],
  ].map((invalid) => [invalid]),
)('rejects invalid boards before fetching: %j', async (invalid) => {
  const fetch = vi.fn();
  vi.stubGlobal('fetch', fetch);
  expect(gamePlanBoardKey(invalid)).toBeUndefined();
  await expect(loadGamePlans(invalid)).rejects.toThrow();
  expect(fetch).not.toHaveBeenCalled();
});

it('uses checker placement alone as a cache key and cannot be poisoned by response mutation', () => {
  const cache = new GamePlanCache();
  const original = { ...plans };
  cache.set(board(), original);
  original.white = 'attacking';
  expect(cache.get([...board()])).toEqual(plans);
  expect(Object.isFrozen(cache.get(board()))).toBe(true);
  expect(() => cache.set(board(), { ...plans, white: 'invented' })).toThrow();
  expect(cache.get(board())).toEqual(plans);
});

it('bounds its cache and evicts the least recently used board', () => {
  const cache = new GamePlanCache(2);
  const a = board(),
    b = board(),
    c = board();
  b[24] = 3;
  c[24] = 4;
  cache.set(a, plans);
  cache.set(b, plans);
  expect(cache.get(a)).toEqual(plans);
  cache.set(c, plans);
  expect(cache.get(b)).toBeUndefined();
  expect(cache.get(a)).toEqual(plans);
  expect(cache.get(c)).toEqual(plans);
  cache.clear();
  expect(cache.get(a)).toBeUndefined();
});
