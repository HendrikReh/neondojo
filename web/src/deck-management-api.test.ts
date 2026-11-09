import { afterEach, expect, it, vi } from 'vitest';
import { deleteDeck, renameDeck, isDeckConflict } from './deck-management-api';

const deck = {
  id: 'deck-a',
  name: 'Practice',
  card_count: 4,
  due_count: 0,
  version: 7,
  created_at: '2026-11-09T00:00:00Z',
};
afterEach(() => vi.unstubAllGlobals());
it('renames the same deck with its observed version and a trimmed name', async () => {
  const fetch = vi
    .fn()
    .mockResolvedValue(Response.json({ ...deck, name: 'Renamed', version: 8 }));
  vi.stubGlobal('fetch', fetch);
  expect(await renameDeck(deck, '  Renamed  ')).toMatchObject({
    id: deck.id,
    name: 'Renamed',
    version: 8,
  });
  expect(fetch).toHaveBeenCalledWith('/api/decks/deck-a', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name: 'Renamed', expected_version: 7 }),
  });
});
it('deletes with the observed version and accepts an empty 204 response', async () => {
  const fetch = vi.fn().mockResolvedValue(new Response(null, { status: 204 }));
  vi.stubGlobal('fetch', fetch);
  await expect(deleteDeck(deck)).resolves.toBeUndefined();
  expect(fetch).toHaveBeenCalledWith('/api/decks/deck-a?expected_version=7', {
    method: 'DELETE',
  });
});
it.each([404, 409])(
  'retains a recoverable %s response without retrying a mutation',
  async (status) => {
    const fetch = vi
      .fn()
      .mockResolvedValue(Response.json({ error: 'deck changed' }, { status }));
    vi.stubGlobal('fetch', fetch);
    const error = await renameDeck(deck, 'Draft').catch((error) => error);
    expect(isDeckConflict(error)).toBe(true);
    expect(error).toMatchObject({ params: { status: String(status) } });
    expect(fetch).toHaveBeenCalledTimes(1);
  },
);
it('does not classify validation or network failures as concurrent changes', async () => {
  vi.stubGlobal('fetch', async () =>
    Response.json({ error: 'invalid' }, { status: 422 }),
  );
  const error = await renameDeck(deck, '').catch((error) => error);
  expect(isDeckConflict(error)).toBe(false);
});
