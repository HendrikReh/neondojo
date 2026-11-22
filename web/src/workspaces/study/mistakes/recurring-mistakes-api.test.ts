import { afterEach, expect, test, vi } from 'vitest';
import {
  loadMistakes,
  loadMistakePractice,
  revealMistake,
} from './recurring-mistakes-api';
afterEach(() => vi.unstubAllGlobals());
test('exact player names and optional filters are sent without widening the selection', async () => {
  const fetch = vi.fn().mockResolvedValue(
    new Response(
      JSON.stringify({
        players: [],
        groups: [],
        analyzed_decisions: 0,
        analyzed_matches: 0,
      }),
    ),
  );
  vi.stubGlobal('fetch', fetch);
  const signal = new AbortController().signal;
  await loadMistakes(
    {
      player: ' Alice ',
      game_plan: 'priming',
      min_loss: 0.08,
      database_id: 'database',
    },
    signal,
  );
  const [url, init] = fetch.mock.calls[0];
  expect(new URL(url, 'http://localhost').searchParams.get('player')).toBe(
    ' Alice ',
  );
  expect(new URL(url, 'http://localhost').searchParams.get('game_plan')).toBe(
    'priming',
  );
  expect(init.signal).toBe(signal);
});
test('practice sends the chosen group and reveal sends only the saved reference', async () => {
  const fetch = vi
    .fn()
    .mockImplementation(() => Promise.resolve(new Response('{}')));
  vi.stubGlobal('fetch', fetch);
  const signal = new AbortController().signal;
  await loadMistakePractice(
    {
      player: 'Alice',
      game_plan: 'priming',
      mistake_kind: 'checker',
      min_loss: 0.08,
    },
    signal,
  );
  const reference = { job_id: 'job', game_no: 2, decision_no: 3 };
  await revealMistake(reference, signal);
  expect(fetch.mock.calls[0][0]).toBe('/api/study/mistakes/practice');
  expect(JSON.parse(fetch.mock.calls[0][1].body)).toEqual({
    player: 'Alice',
    game_plan: 'priming',
    mistake_kind: 'checker',
    min_loss: 0.08,
  });
  expect(fetch.mock.calls[1][0]).toBe('/api/study/mistakes/reveal');
  expect(JSON.parse(fetch.mock.calls[1][1].body)).toEqual(reference);
  expect(fetch.mock.calls[1][1].signal).toBe(signal);
});
