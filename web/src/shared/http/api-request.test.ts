import { afterEach, expect, test, vi } from 'vitest';
import { HttpError, isConflict, request } from './api';

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

async function failure(response: Response | Error) {
  vi.stubGlobal('fetch', async () => {
    if (response instanceof Error) throw response;
    return response;
  });
  return request('/x').then(
    () => undefined,
    (error: unknown) => error,
  );
}
test('keeps the status, code and issues of a refused request', async () => {
  const issue = { path: 'play.engine', code: 'invalid_value', message: 'No' };
  const error = await failure(
    Response.json(
      { error: 'Refused', code: 'invalid_configuration', issues: [issue, 7] },
      { status: 422 },
    ),
  );
  expect(error).toBeInstanceOf(HttpError);
  expect(error).toMatchObject({
    status: 422,
    code: 'invalid_configuration',
    issues: [issue],
    diagnostic: 'Refused',
  });
  expect(isConflict(error)).toBe(false);
});
test('reads a refusal without a JSON error body', async () => {
  expect(
    await failure(new Response('Bad gateway', { status: 502 })),
  ).toMatchObject({
    status: 502,
    code: '',
    issues: [],
    diagnostic: 'Bad gateway',
  });
  expect(
    await failure(Response.json({ error: 'Old' }, { status: 409 })),
  ).toSatisfy(isConflict);
});
test('reports a lost connection and passes cancellation through', async () => {
  expect(await failure(new TypeError('Failed to fetch'))).toMatchObject({
    key: 'error.network',
    diagnostic: 'Failed to fetch',
  });
  const abort = new DOMException('Cancelled', 'AbortError');
  expect(await failure(abort)).toBe(abort);
});

test('only the shared helper calls fetch', () => {
  const sources = import.meta.glob<string>('../../**/*.{ts,tsx}', {
    eager: true,
    query: '?raw',
    import: 'default',
  });
  expect(
    Object.entries(sources)
      .filter(
        ([path, source]) =>
          !/\.test\.|\.d\.ts$/.test(path) && /\bfetch\(/.test(source),
      )
      .map(([path]) => path)
      .sort(),
    // The board loads its sounds, which are not part of the API.
  ).toEqual(['../board/play-audio.ts', './api.ts']);
});

test('distinguishes background indexing from a generic service failure', async () => {
  const error = await failure(
    Response.json(
      {
        code: 'index_preparing',
        error: 'The decision index is being prepared.',
      },
      { status: 503 },
    ),
  );
  expect(error).toMatchObject({
    key: 'error.indexPreparing',
    code: 'index_preparing',
    status: 503,
  });
});
