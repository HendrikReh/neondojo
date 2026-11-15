import { expect, it, vi } from 'vitest';
import { positionJobsPath, useJobs } from './useJobs';

const hooks = vi.hoisted(() => ({
  engine: 'gnubg',
  index: 0,
  state: [] as unknown[],
  effects: [] as Array<() => void>,
}));
vi.mock('./ConfigurationProvider', () => ({
  useOptionalConfiguration: () => ({
    current: { analysis: { engine: hooks.engine } },
  }),
}));
vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useRef: (initial: unknown) =>
    (hooks.state[hooks.index++] ??= { current: initial }),
  useState(initial: unknown) {
    const index = hooks.index++;
    if (!(index in hooks.state)) hooks.state[index] = initial;
    return [
      hooks.state[index],
      (value: unknown) => {
        hooks.state[index] =
          typeof value === 'function' ? value(hooks.state[index]) : value;
      },
    ];
  },
  useEffect(effect: () => void) {
    hooks.effects.push(effect);
  },
}));

it('discovers persisted jobs for the configured engine without browser storage', () => {
  expect(positionJobsPath('saved position', 'bgsage')).toBe(
    '/positions/saved%20position/jobs?engine_id=bgsage',
  );
});

it('discards dice results from the previous engine even before effect cleanup', async () => {
  const nativeDice = {
    id: 'native-dice',
    position_uuid: 'position',
    engine_snapshot: { engine_id: 'gnubg' },
    status: 'completed',
    kind: 'dice',
    plies: 2,
  };
  const sageDice = {
    ...nativeDice,
    id: 'sage-dice',
    engine_snapshot: { engine_id: 'bgsage' },
  };
  const fetch = vi
    .fn<typeof globalThis.fetch>()
    .mockImplementationOnce(async () => Response.json([nativeDice]))
    .mockImplementationOnce(async () => Response.json([sageDice, nativeDice]));
  vi.stubGlobal('fetch', fetch);
  const render = () => {
    hooks.index = 0;
    hooks.effects = [];
    return useJobs('position');
  };
  const commit = async () => {
    hooks.effects.forEach((effect) => effect());
    await new Promise((resolve) => setTimeout(resolve, 0));
  };
  try {
    render();
    await commit();
    expect(render().jobs.dice?.id).toBe('native-dice');

    hooks.engine = 'bgsage';
    expect(render().jobs.dice).toBeUndefined();
    await commit();
    expect(fetch.mock.calls.map(([url]) => url)).toEqual([
      '/api/positions/position/jobs?engine_id=gnubg',
      '/api/positions/position/jobs?engine_id=bgsage',
    ]);
    expect(render().jobs.dice?.id).toBe('sage-dice');
  } finally {
    vi.unstubAllGlobals();
  }
});
