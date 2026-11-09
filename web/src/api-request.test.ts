import { afterEach, expect, test, vi } from 'vitest';
import { request } from './api';

afterEach(() => vi.unstubAllGlobals());
test('accepts an explicit successful no-content deletion', async () => {
  vi.stubGlobal('fetch', async () => new Response(null, { status: 204 }));
  await expect(
    request<void>('/positions/id/bookmark', { method: 'DELETE' }),
  ).resolves.toBeUndefined();
});
test('still rejects empty JSON successes and retains server deletion errors', async () => {
  vi.stubGlobal('fetch', async () => new Response('', { status: 200 }));
  await expect(request('/positions/id/bookmark')).rejects.toMatchObject({
    key: 'error.invalidResponse',
  });
  vi.stubGlobal('fetch', async () =>
    Response.json({ error: 'Position not found' }, { status: 404 }),
  );
  await expect(
    request('/positions/id/bookmark', { method: 'DELETE' }),
  ).rejects.toMatchObject({ key: 'error.request', params: { status: '404' } });
});
