import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { AppError } from '../../../shared/i18n/i18n-core';
import { ALL_CARDS } from './card-search-api';
import type { VersionedDeck } from './deck-management-api';
import type {
  AttemptedCard,
  NextCard,
  RevealedCard,
  ReviewResult,
} from './learning-api';

const hooks = vi.hoisted(() => ({
  slots: [] as unknown[],
  index: 0,
  pending: new Map<number, () => (() => void) | void>(),
  request: vi.fn(),
}));

interface EffectSlot {
  deps: unknown[];
  cleanup?: (() => void) | void;
}

vi.mock('react', async (original) => ({
  ...(await original<typeof import('react')>()),
  useInsertionEffect(effect: () => void) {
    effect();
  },
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

vi.mock('../../../shared/http/api', () => ({
  request: hooks.request,
}));

import { useFsrsReviewSession } from './useFsrsReviewSession';

const decks: VersionedDeck[] = [
  {
    id: 'deck-a',
    name: 'Race',
    card_count: 5,
    due_count: 2,
    version: 1,
    created_at: '2026-11-01T00:00:00Z',
  },
  {
    id: 'deck-b',
    name: 'Cube',
    card_count: 3,
    due_count: 1,
    version: 1,
    created_at: '2026-11-01T00:00:00Z',
  },
];

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

const nextCard = (id: string, version = 1): NextCard => ({
  card: {
    id,
    version,
    kind: 'checker',
    players: ['Ada', 'Bob'],
    prompt: 'Best checker play?',
    due_at: '2026-11-27T00:00:00Z',
    review_count: 0,
    tags_version: 0,
    position: null,
    source: {
      kind: 'analysis',
      job_id: null,
      match_id: 'match-1',
      position_id: null,
      game_no: 1,
      decision_no: null,
    },
  },
  counts: { total: 5, due: 2, new: 1 },
  active_attempt: null,
});

const revealedCard = (card_id: string, version = 1): RevealedCard => ({
  card_id,
  version,
  session_id: `session-${card_id}`,
  answer: {
    played: '8/5 6/5',
    solution: { candidates: [], decision: null, engine: 'GNUbg', plies: 2 },
    notes: '',
    tags: [],
  },
  intervals: { again: 60, hard: 300, good: 600, easy: 3600 },
});

function commit() {
  for (const [index, effect] of hooks.pending) {
    const slot = hooks.slots[index] as EffectSlot;
    slot.cleanup?.();
    slot.cleanup = effect();
  }
  hooks.pending.clear();
}

function settle() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

beforeEach(() => {
  hooks.slots = [];
  hooks.index = 0;
  hooks.pending.clear();
  hooks.request.mockReset();
});

afterEach(() => {
  for (const slot of hooks.slots) {
    if (slot && typeof slot === 'object' && 'cleanup' in slot) {
      (slot as EffectSlot).cleanup?.();
    }
  }
});

function renderSession(
  overrides: Partial<Parameters<typeof useFsrsReviewSession>[0]> = {},
) {
  hooks.index = 0;
  return useFsrsReviewSession({
    deckId: 'deck-a',
    decks,
    settled: true,
    onSelectDeck: vi.fn(),
    onClearPractice: vi.fn(),
    onChanged: vi.fn(),
    ...overrides,
  });
}

it('loads next card on startStudy, records valid attempts, and fences stale responses across deck switches', async () => {
  const firstDeferred = deferred<NextCard>();
  hooks.request.mockReturnValueOnce(firstDeferred.promise);

  let state = renderSession({ deckId: 'deck-a' });
  commit();
  state.startStudy();
  renderSession({ deckId: 'deck-a' });
  commit();

  expect(hooks.request).toHaveBeenCalledWith(
    '/decks/deck-a/next',
    expect.objectContaining({ signal: expect.any(AbortSignal) }),
  );
  const firstSignal = hooks.request.mock.calls[0][1].signal as AbortSignal;

  // Switching decks before first response resolves fences the stale card.
  const secondDeferred = deferred<NextCard>();
  hooks.request.mockReturnValueOnce(secondDeferred.promise);
  renderSession({ deckId: 'deck-b' });
  firstDeferred.resolve(nextCard('card-from-a'));
  await settle();
  expect(renderSession({ deckId: 'deck-b' }).activeNext).toBeNull();
  commit();
  expect(firstSignal.aborted).toBe(true);

  secondDeferred.resolve(nextCard('card-from-b', 2));
  await settle();
  state = renderSession({ deckId: 'deck-b' });
  expect(state.activeNext?.card?.id).toBe('card-from-b');

  // Matching attempt attaches; mismatched version is ignored.
  const recordAttempt = state.beginAttempt();
  const attempt: AttemptedCard = {
    submitted: {
      kind: 'checker',
      steps: [
        { from: 8, to: 5 },
        { from: 6, to: 5 },
      ],
    },
    evaluation: {
      verdict: 'best',
      chosen: '8/5 6/5',
      best: '8/5 6/5',
      chosen_equity: 0.25,
      best_equity: 0.25,
      equity_loss: 0,
      provenance: { kind: 'stored', engine: 'GNUbg', plies: 2 },
    },
    revealed: revealedCard('card-from-b', 2),
  };
  recordAttempt(attempt);
  expect(
    renderSession({ deckId: 'deck-b' }).activeNext?.active_attempt,
  ).toEqual(attempt);
});

it('reveals, retries grade with a stable request_id, advances queue, and reloads on 409 conflict', async () => {
  const onChanged = vi.fn();
  const card = nextCard('card-1', 4);
  const answer = revealedCard('card-1', 4);
  const reviewResult: ReviewResult = {
    card_id: 'card-1',
    due_at: '2026-11-28T00:00:00Z',
    review_count: 1,
    difficulty: 5.0,
    stability: 1.5,
    interval_seconds: 86400,
    version: 5,
  };

  hooks.request
    .mockResolvedValueOnce(card) // initial /next
    .mockResolvedValueOnce(answer) // /reveal
    .mockRejectedValueOnce(new Error('temporary offline')) // 1st /review fails
    .mockResolvedValueOnce(reviewResult) // retry /review succeeds
    .mockResolvedValueOnce(nextCard('card-2', 1)); // next card after grade

  let state = renderSession({ onChanged });
  commit();
  state.startStudy();
  renderSession({ onChanged });
  commit();
  await settle();

  state = renderSession({ onChanged });
  await state.reveal();
  state = renderSession({ onChanged });
  expect(state.revealed).toEqual(answer);

  // First grade fails -> error stored; retrying same rating reuses request_id.
  await state.grade(answer, 'good');
  state = renderSession({ onChanged });
  expect(state.error).toBeInstanceOf(Error);
  const firstBody = JSON.parse(hooks.request.mock.calls[2][1].body as string);

  await state.grade(answer, 'good');
  const retryBody = JSON.parse(hooks.request.mock.calls[3][1].body as string);
  expect(retryBody.request_id).toBe(firstBody.request_id);
  expect(retryBody.session_id).toBe(answer.session_id);
  expect(onChanged).toHaveBeenCalledTimes(1);

  state = renderSession({ onChanged });
  expect(state.reviewResult).toEqual(reviewResult);
  expect(state.revealed).toBeNull();
  commit();
  await settle();
  expect(renderSession({ onChanged }).activeNext?.card?.id).toBe('card-2');

  // 409 on reveal reloads the queue instead of setting an error.
  hooks.request
    .mockRejectedValueOnce(new AppError('error.request', { status: '409' }))
    .mockResolvedValueOnce(nextCard('card-2', 2));
  state = renderSession({ onChanged });
  await state.reveal();
  state = renderSession({ onChanged });
  expect(state.error).toBeNull();
  commit();
  await settle();
  expect(renderSession({ onChanged }).activeNext?.card?.version).toBe(2);
});

it('handles requestedDeck for missing decks, 404 next responses, and deleted-deck resets', async () => {
  const onSelectDeck = vi.fn();
  const onClearPractice = vi.fn();
  const onChanged = vi.fn();

  // Unknown requestedDeck marks missingRequestedDeck and leaves study inactive.
  renderSession({
    requestedDeck: { id: 'missing-deck', token: 1 },
    onSelectDeck,
    onClearPractice,
    onChanged,
  });
  commit();
  let state = renderSession({
    requestedDeck: { id: 'missing-deck', token: 1 },
    onSelectDeck,
    onClearPractice,
    onChanged,
  });
  expect(state.missingRequestedDeck).toBe(true);
  expect(state.studying).toBe(false);
  expect(onClearPractice).toHaveBeenCalledTimes(1);

  // Valid requestedDeck selects it and starts study; 404 on /next falls back to ALL_CARDS.
  hooks.request.mockRejectedValueOnce(
    new AppError('error.request', { status: '404' }),
  );
  renderSession({
    deckId: 'deck-b',
    requestedDeck: { id: 'deck-b', token: 2 },
    onSelectDeck,
    onClearPractice,
    onChanged,
  });
  commit();
  renderSession({
    deckId: 'deck-b',
    requestedDeck: { id: 'deck-b', token: 2 },
    onSelectDeck,
    onClearPractice,
    onChanged,
  });
  commit();
  await settle();

  expect(onSelectDeck).toHaveBeenCalledWith('deck-b');
  expect(onSelectDeck).toHaveBeenCalledWith(ALL_CARDS);
  expect(onChanged).toHaveBeenCalledTimes(1);
  state = renderSession({
    deckId: ALL_CARDS,
    requestedDeck: { id: 'deck-b', token: 2 },
    onSelectDeck,
    onClearPractice,
    onChanged,
  });
  expect(state.missingRequestedDeck).toBe(true);
  expect(state.studying).toBe(false);

  // resetAfterDeletedDeck clears study state and invalidates in-flight session id.
  state.resetAfterDeletedDeck();
  expect(state.liveSession.current).toBe(-1);
});
