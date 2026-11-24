import { expect, it } from 'vitest';
import type { PlayDecision, PlaySession } from './play-api';
import { playFeedback, playInterruption } from './play-interruption';

const session = (value: Partial<PlaySession>) =>
  ({ match_length: 7, phase: 'move', ...value }) as PlaySession;
const decision = (value: Partial<PlayDecision>) => value as PlayDecision;
const interruption = (
  value: Partial<PlaySession>,
  state: Partial<Parameters<typeof playInterruption>[0]> = {},
) =>
  playInterruption({
    session: session(value),
    blocked: false,
    resignOpen: false,
    hintOpen: false,
    coaching: false,
    acknowledged: false,
    ...state,
  });

it('interrupts for cube offers, feedback and results', () => {
  expect(interruption({})).toBeNull();
  expect(interruption({ phase: 'double_response' })).toBe('cube');
  expect(interruption({ phase: 'game_over' })).toBe('game_over');
  expect(
    interruption({
      play_mode: 'tutor',
      pending_decision: decision({ action: 'move' }),
    }),
  ).toBe('feedback');
  // A cube offer shows its own feedback.
  expect(
    interruption({
      phase: 'double_response',
      play_mode: 'tutor',
      pending_decision: decision({ action: 'take' }),
    }),
  ).toBe('cube');
});

it('shows a match result until it is acknowledged', () => {
  const over = { phase: 'match_over' } as const;
  expect(interruption(over)).toBe('match_over');
  expect(interruption(over, { acknowledged: true })).toBeNull();
});

it('offers the next money game until the session ends', () => {
  const money = { match_length: 0, phase: 'match_over' } as const;
  expect(interruption(money)).toBe('game_over');
  expect(interruption({ ...money, money_session_ended: true })).toBe(
    'match_over',
  );
});

it('lets nothing interrupt a blocked board and puts opened dialogs first', () => {
  const offer = { phase: 'double_response', play_mode: 'coach' } as const;
  expect(interruption(offer, { blocked: true, resignOpen: true })).toBeNull();
  expect(interruption(offer, { resignOpen: true, hintOpen: true })).toBe(
    'resign',
  );
  expect(interruption(offer, { hintOpen: true, coaching: true })).toBe('hint');
  expect(interruption(offer, { coaching: true })).toBeNull();
  // Competition has no hints.
  expect(
    interruption({ ...offer, play_mode: 'competition' }, { hintOpen: true }),
  ).toBe('cube');
});

it.each([
  ['training', {}, true],
  ['training', { warning: true }, false],
  ['coach', { severity: 'none' }, true],
  ['coach', { severity: 'bad' }, false],
  ['tutor', { severity: 'none' }, false],
] as const)(
  'a %s roll with %j continues without feedback: %s',
  (play_mode, extra, routine) => {
    const result = playFeedback(
      session({
        play_mode,
        pending_decision: decision({
          action: 'roll',
          ...(extra as Partial<PlayDecision>),
        }),
      }),
    );
    expect(Boolean(result.routineRoll)).toBe(routine);
    expect(result.awaiting).toBe(!routine);
  },
);
