import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Match } from './api';
import type { LibraryMatch } from './library-api';

// Commit effects separately so navigation/revision changes can render before
// React cleans up a request that has already received its response.
const hooks = vi.hoisted(() => ({
  slots: [] as unknown[],
  index: 0,
  pending: new Map<number, () => (() => void) | void>(),
}));
interface EffectSlot {
  deps: unknown[];
  cleanup?: (() => void) | void;
}
vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useState(initial: unknown) {
    const index = hooks.index++;
    if (!(index in hooks.slots))
      hooks.slots[index] = typeof initial === 'function' ? initial() : initial;
    return [
      hooks.slots[index],
      (next: unknown) => {
        hooks.slots[index] =
          typeof next === 'function' ? next(hooks.slots[index]) : next;
      },
    ];
  },
  useRef(initial: unknown) {
    const index = hooks.index++;
    return (hooks.slots[index] ??= { current: initial });
  },
  useEffect(effect: () => (() => void) | void, deps: unknown[]) {
    const index = hooks.index++;
    const previous = hooks.slots[index] as EffectSlot | undefined;
    if (!previous || deps.some((dep, i) => !Object.is(dep, previous.deps[i]))) {
      hooks.slots[index] = { deps, cleanup: previous?.cleanup };
      hooks.pending.set(index, effect);
    }
  },
}));
import AnalysisMatchHeader from './AnalysisMatchHeader';

type Props = Parameters<typeof AnalysisMatchHeader>[0];
const match: Match & { source_available: boolean } = {
  id: 'match-one',
  white: 'Alice',
  black: 'Bob',
  match_length: 21,
  filename: 'match.xg',
  format: 'xg',
  position_count: 0,
  created_at: '',
  source_available: true,
};
const metadata: LibraryMatch = {
  ...match,
  match_no: 7,
  database_id: '00000000-0000-0000-0000-000000000001',
  display_name: 'Retained test',
  comment: '',
  game_count: 15,
  played_on: null,
  tournament: null,
  round: null,
  place: null,
  result_state: 'completed',
  white_score: 21,
  black_score: 17,
  white_pr: 2.75,
  black_pr: 4.5,
  pr_method: null,
  pr_profile: null,
  analysis_status: 'completed',
  tags: [],
  version: 0,
};
const fetch = vi.fn<typeof globalThis.fetch>();
function render(props: Partial<Props> = {}) {
  hooks.index = 0;
  return AnalysisMatchHeader({ match, ...props }).props;
}
function commit() {
  for (const [index, effect] of hooks.pending) {
    const slot = hooks.slots[index] as EffectSlot;
    slot.cleanup?.();
    slot.cleanup = effect();
  }
  hooks.pending.clear();
}
async function settle() {
  for (let i = 0; i < 16; i++) await Promise.resolve();
}
function deferred() {
  let resolve!: (response: Response) => void;
  const promise = new Promise<Response>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
async function load() {
  fetch.mockResolvedValueOnce(Response.json(metadata));
  render();
  commit();
  await settle();
  return render();
}
beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
  fetch.mockReset();
  vi.stubGlobal('fetch', fetch);
});
afterEach(() => {
  for (const slot of hooks.slots) (slot as EffectSlot | undefined)?.cleanup?.();
  vi.unstubAllGlobals();
});

it('loads selected metadata once through the match endpoint without a library scan', async () => {
  const loaded = await load();
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(fetch).toHaveBeenCalledWith('/api/matches/match-one/metadata', {
    signal: expect.any(AbortSignal),
  });
  expect(loaded).toMatchObject({
    match,
    metadata,
    loading: false,
    error: undefined,
  });
  render();
  commit();
  expect(fetch).toHaveBeenCalledTimes(1);
});

it('clears old PR immediately on match selection before effect cleanup and ignores the old response', async () => {
  const old = deferred();
  fetch.mockReturnValueOnce(old.promise);
  render();
  commit();
  const second = { ...match, id: 'match-two', white: 'Carol' };
  expect(render({ match: second })).toMatchObject({
    loading: true,
    metadata: undefined,
  });
  old.resolve(Response.json(metadata));
  await settle();
  expect(render({ match: second }).metadata).toBeUndefined();
  fetch.mockResolvedValueOnce(
    Response.json({ ...metadata, ...second, match_no: 8, white_pr: null }),
  );
  commit();
  expect((fetch.mock.calls[0][1]?.signal as AbortSignal).aborted).toBe(true);
  await settle();
  expect(render({ match: second }).metadata).toMatchObject({
    id: 'match-two',
    match_no: 8,
    white_pr: null,
  });
});

it('hides loaded metadata immediately and refetches after a database edit', async () => {
  await load();
  const props = { metadataRevision: 1 };
  const pending = render(props);
  expect(pending).toMatchObject({ loading: true, metadata: undefined });
  fetch.mockResolvedValueOnce(
    Response.json({ ...metadata, tournament: 'Edited event', version: 1 }),
  );
  commit();
  await settle();
  expect(render(props).metadata).toMatchObject({
    tournament: 'Edited event',
    version: 1,
  });
  expect(fetch).toHaveBeenCalledTimes(2);
});

it('fences the old revision response even before its effect is cleaned up', async () => {
  const old = deferred();
  fetch.mockReturnValueOnce(old.promise);
  render();
  commit();
  const props = { metadataRevision: 1 };
  render(props);
  old.resolve(Response.json(metadata));
  await settle();
  expect(render(props)).toMatchObject({ loading: true, metadata: undefined });
});

it('refreshes full-match PR when a completed analysis revision arrives', async () => {
  await load();
  const props = { analysisRevision: 'completed-match:2026-11-09T12:00:00Z' };
  expect(render(props)).toMatchObject({ loading: true, metadata: undefined });
  fetch.mockResolvedValueOnce(
    Response.json({ ...metadata, white_pr: 0, black_pr: 3.25 }),
  );
  commit();
  await settle();
  expect(render(props).metadata).toMatchObject({ white_pr: 0, black_pr: 3.25 });
  expect(fetch).toHaveBeenCalledTimes(2);
});

it('exposes a request error and retries loading without stale values', async () => {
  fetch.mockResolvedValueOnce(
    Response.json({ error: 'Offline' }, { status: 503 }),
  );
  render();
  commit();
  await settle();
  const failed = render();
  expect(failed.loading).toBe(false);
  expect(failed.metadata).toBeUndefined();
  expect(failed.error).toBeInstanceOf(Error);
  failed.onRetry();
  expect(render()).toMatchObject({
    loading: true,
    metadata: undefined,
    error: undefined,
  });
  fetch.mockResolvedValueOnce(Response.json(metadata));
  commit();
  await settle();
  expect(render()).toMatchObject({
    metadata,
    loading: false,
    error: undefined,
  });
});

it('rejects metadata belonging to a different match', async () => {
  fetch.mockResolvedValueOnce(
    Response.json({ ...metadata, id: 'wrong-match' }),
  );
  render();
  commit();
  await settle();
  const failed = render();
  expect(failed.metadata).toBeUndefined();
  expect(failed.loading).toBe(false);
  expect(failed.error).toBeInstanceOf(Error);
});
