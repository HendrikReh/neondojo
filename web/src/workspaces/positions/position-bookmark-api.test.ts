import { afterEach, expect, it, vi } from 'vitest';
import {
  loadPositionBookmark,
  removePositionBookmark,
  savePositionBookmark,
} from './position-bookmark-api';

afterEach(() => vi.unstubAllGlobals());

it('loads membership for the exact source occurrence and forwards cancellation', async () => {
  const status = { position_id: 'source-id', saved: false, saved_at: null };
  const fetch = vi
    .fn<typeof globalThis.fetch>()
    .mockResolvedValue(Response.json(status));
  vi.stubGlobal('fetch', fetch);
  const abort = new AbortController();
  expect(await loadPositionBookmark('source-id', abort.signal)).toEqual(status);
  expect(fetch).toHaveBeenCalledWith('/api/positions/source-id/bookmark', {
    signal: abort.signal,
  });
});

it('explicitly saves using PUT and preserves the server timestamp', async () => {
  const status = {
    position_id: 'source/id',
    saved: true,
    saved_at: '2026-11-09T10:00:00Z',
  };
  const fetch = vi
    .fn<typeof globalThis.fetch>()
    .mockResolvedValue(Response.json(status));
  vi.stubGlobal('fetch', fetch);
  expect(await savePositionBookmark('source/id')).toEqual(status);
  expect(fetch).toHaveBeenCalledWith('/api/positions/source%2Fid/bookmark', {
    method: 'PUT',
    signal: undefined,
  });
});

it('removes membership with DELETE and accepts an empty successful response', async () => {
  const fetch = vi
    .fn<typeof globalThis.fetch>()
    .mockResolvedValue(new Response(null, { status: 204 }));
  vi.stubGlobal('fetch', fetch);
  await expect(removePositionBookmark('source-id')).resolves.toBeUndefined();
  expect(fetch).toHaveBeenCalledWith('/api/positions/source-id/bookmark', {
    method: 'DELETE',
    signal: undefined,
  });
});

it('keeps failed writes as errors instead of inventing membership', async () => {
  vi.stubGlobal('fetch', async () =>
    Response.json({ error: 'Unavailable' }, { status: 503 }),
  );
  await expect(savePositionBookmark('source-id')).rejects.toMatchObject({
    params: { status: '503' },
  });
});
