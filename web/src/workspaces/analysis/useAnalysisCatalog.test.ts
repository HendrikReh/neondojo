import { afterEach, beforeEach, expect, it, vi } from 'vitest';
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
    return (hooks.slots[hooks.index++] ??= { current: initial });
  },
  useEffect(effect: () => (() => void) | void, deps: unknown[]) {
    const index = hooks.index++;
    const prior = hooks.slots[index] as EffectSlot | undefined;
    if (!prior || deps.some((value, i) => !Object.is(value, prior.deps[i]))) {
      hooks.slots[index] = { deps, cleanup: prior?.cleanup };
      hooks.pending.set(index, effect);
    }
  },
}));
import { useAnalysisCatalog, useAnalysisResult } from './useAnalysisCatalog';
import type { AnalysisCatalogPage } from './analysis-catalog-api';
import { entryForJob } from './analysis-catalog-test-fixtures';
import type { Job } from '../../shared/http/api';
const fetch = vi.fn<typeof globalThis.fetch>();
const entry = entryForJob({
  id: 'native',
  plies: 2,
  status: 'completed',
} as Job);
const page: AnalysisCatalogPage = {
  match_id: 'match',
  source_revision: 0,
  items: [entry],
  selected: entry,
  selected_id: null,
  next_cursor: 'next',
};
function commit() {
  for (const [index, effect] of hooks.pending) {
    const slot = hooks.slots[index] as EffectSlot;
    slot.cleanup?.();
    slot.cleanup = effect();
  }
  hooks.pending.clear();
}
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
function render(matchId = 'match', positionId = 'one') {
  hooks.index = 0;
  return useAnalysisCatalog(matchId, positionId);
}
function response(body: unknown) {
  return new Response(JSON.stringify(body), {
    headers: { 'Content-Type': 'application/json' },
  });
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
it('loads server selection without browser storage and persists explicit choices', async () => {
  fetch.mockResolvedValueOnce(response(page));
  render();
  commit();
  await tick();
  expect(render().analysisId).toBe(entry.id);
  fetch.mockResolvedValueOnce(
    response({ match_id: 'match', analysis_id: 'imported:match' }),
  );
  await render().select('imported:match');
  const [url, options] = fetch.mock.calls[1];
  expect(url).toBe('/api/matches/match/analysis-selection');
  expect(options?.method).toBe('PUT');
  expect(JSON.parse(options!.body as string)).toEqual({
    analysis_id: 'imported:match',
  });
  expect(render().analysisId).toBeUndefined();
});
it('keeps an old page response from replacing a newer persisted selection', async () => {
  fetch.mockResolvedValueOnce(response(page));
  render();
  commit();
  await tick();
  let finishPage!: (response: Response) => void;
  fetch.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finishPage = resolve;
      }),
  );
  const pendingPage = render().loadMore();
  fetch.mockResolvedValueOnce(
    response({ match_id: 'match', analysis_id: 'imported:match' }),
  );
  await render().select('imported:match');
  finishPage(response(page));
  await pendingPage;
  expect(render().analysisId).toBeUndefined();
});
it('does not expose previous-position evaluations before effect cleanup', async () => {
  fetch.mockResolvedValueOnce(
    response({
      analysis_id: 'job:native',
      position_id: 'one',
      engine: 'GNUbg',
      depth: '2-ply',
      records: [],
      analysis: null,
      decision: null,
    }),
  );
  hooks.index = 0;
  useAnalysisResult('one', 'job:native');
  commit();
  await tick();
  hooks.index = 0;
  expect(useAnalysisResult('one', 'job:native').result?.position_id).toBe(
    'one',
  );
  hooks.index = 0;
  expect(useAnalysisResult('two', 'job:native').result).toBeUndefined();
  hooks.index = 0;
  expect(useAnalysisResult('one', 'imported:match').result).toBeUndefined();
});

it('updates polled entries and preserves already loaded older pages', async () => {
  vi.useFakeTimers();
  try {
    fetch.mockResolvedValueOnce(
      response({ ...page, items: [{ ...entry, status: 'running' }] }),
    );
    render();
    commit();
    await vi.advanceTimersByTimeAsync(0);
    const older = { ...entry, id: 'job:older' };
    fetch.mockResolvedValueOnce(
      response({ ...page, items: [older], next_cursor: null }),
    );
    await render().loadMore();
    fetch.mockResolvedValueOnce(response(page));
    await vi.advanceTimersByTimeAsync(3000);
    expect(render().entries.find((item) => item.id === entry.id)?.status).toBe(
      'completed',
    );
    expect(render().entries.some((item) => item.id === older.id)).toBe(true);
    expect(render().nextCursor).toBeNull();
  } finally {
    vi.useRealTimers();
  }
});

it.each([0, 1])(
  'keeps the latest checkpoint when a delayed older page arrives (source revision %i)',
  async (sourceRevision) => {
    vi.useFakeTimers();
    try {
      const running = entryForJob({
        ...entry.job!,
        status: 'running',
        completed_games: 1,
      });
      fetch.mockResolvedValueOnce(
        response({ ...page, items: [running], selected: running }),
      );
      render();
      commit();
      await vi.advanceTimersByTimeAsync(0);

      let finishPage!: (value: Response) => void;
      fetch.mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finishPage = resolve;
          }),
      );
      const pending = render().loadMore();
      const newer = entryForJob({ ...running.job!, completed_games: 2 });
      fetch.mockResolvedValueOnce(
        response({
          ...page,
          source_revision: sourceRevision,
          items: [newer],
          selected: newer,
        }),
      );
      await vi.advanceTimersByTimeAsync(3000);
      expect(render().selected?.job?.completed_games).toBe(2);

      const older = { ...entry, id: 'job:older' };
      finishPage(
        response({
          ...page,
          items: [older],
          selected: running,
          next_cursor: null,
        }),
      );
      await pending;
      const catalog = render();
      expect(catalog.sourceRevision).toBe(sourceRevision);
      expect(catalog.selected?.job?.completed_games).toBe(2);
      expect(catalog.entries.map((item) => item.id)).toEqual(
        sourceRevision === 0 ? [newer.id, older.id] : [newer.id],
      );
    } finally {
      vi.useRealTimers();
    }
  },
);

it('polls an active displayed analysis outside the first catalog page', async () => {
  vi.useFakeTimers();
  try {
    const running = entryForJob({
      ...entry.job!,
      status: 'running',
      completed_games: 1,
    });
    const visible = { ...entry, id: 'job:visible' };
    fetch.mockResolvedValueOnce(
      response({ ...page, items: [visible], selected: running }),
    );
    render();
    commit();
    await vi.advanceTimersByTimeAsync(0);
    fetch.mockResolvedValueOnce(
      response({
        ...page,
        items: [running],
        selected: running,
        next_cursor: null,
      }),
    );
    await render().loadMore();
    const complete = entryForJob({
      ...running.job!,
      status: 'completed',
      completed_games: 2,
    });
    fetch.mockResolvedValueOnce(
      response({
        ...page,
        items: [visible],
        selected: complete,
      }),
    );
    await vi.advanceTimersByTimeAsync(3000);
    expect(render().selected?.job?.completed_games).toBe(2);
    expect(render().entries.find((item) => item.id === running.id)).toEqual(
      complete,
    );
  } finally {
    vi.useRealTimers();
  }
});

it('refreshes the same match after a selection write finishes on another position', async () => {
  fetch.mockResolvedValueOnce(response(page));
  render();
  commit();
  await tick();
  let finish!: (value: Response) => void;
  fetch.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const saving = render().select('imported:match');
  fetch.mockResolvedValueOnce(response(page));
  const next = render('match', 'two');
  commit();
  await tick();
  expect(next.saving).toBe(true);
  finish(response({ match_id: 'match', analysis_id: 'imported:match' }));
  await saving;
  const imported = { ...entry, id: 'imported:match', source: 'imported' };
  fetch.mockResolvedValueOnce(
    response({ ...page, selected: imported, selected_id: imported.id }),
  );
  render('match', 'two');
  commit();
  await tick();
  expect(render('match', 'two').analysisId).toBe(imported.id);
});
