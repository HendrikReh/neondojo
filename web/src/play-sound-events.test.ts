import { expect, it } from 'vitest';
import {
  checkerSound,
  soundsForAction,
  type SoundState,
} from './play-sound-events';
const board = () => Array<number>(26).fill(0);
function state(values: Partial<SoundState> = {}): SoundState {
  return {
    id: 'match',
    version: 0,
    human_player: 0,
    phase: 'move',
    position: { board: board() },
    history: [],
    ...values,
  };
}
it('distinguishes movement, hits and bear-off for both colors and bar entries', () => {
  const b = board();
  b[4] = -1;
  b[21] = 1;
  expect(checkerSound(b, { from: 6, to: 5 }, 0)).toBe('move');
  expect(checkerSound(b, { from: 6, to: 4 }, 0)).toBe('hit');
  expect(checkerSound(b, { from: 3, to: -1 }, 0)).toBe('bear_off');
  expect(checkerSound(b, { from: 19, to: 21 }, 1)).toBe('hit');
  expect(checkerSound(b, { from: 23, to: 26 }, 1)).toBe('bear_off');
  expect(checkerSound(b, { from: 25, to: 21 }, 0)).toBe('move');
  expect(checkerSound(b, { from: 0, to: 4 }, 1)).toBe('move');
});
it('replays the human native move before classifying bot hits, without repeating human sounds', () => {
  const b = board();
  b[6] = 2;
  b[1] = -1;
  const before = state({ position: { board: b } });
  const after = state({
    version: 1,
    history: [
      { player: 0, dice: [2, 1], notation: '6/4 6/5' },
      { player: 1, dice: [3, 1], notation: '24/21 21/20' },
    ],
  });
  expect(soundsForAction(before, after, 'move')).toEqual([
    'roll',
    'hit',
    'hit',
  ]);
});
it('maps black bot bearing off and skips cube records while preserving checker order', () => {
  const b = board();
  b[24] = -2;
  const before = state({
    position: { board: b },
    history: [{ player: 0, dice: [], notation: 'double' }],
  });
  const after = state({
    version: 1,
    history: [
      ...before.history,
      { player: 1, dice: [], notation: 'take' },
      { player: 1, dice: [2, 1], notation: '1/off 1/off' },
    ],
  });
  expect(soundsForAction(before, after, 'take')).toEqual([
    'roll',
    'bear_off',
    'bear_off',
  ]);
});
it('maps white bot bar entry and ignores forced passes as checker sounds', () => {
  const b = board();
  b[25] = 1;
  b[24] = -1;
  const before = state({ human_player: 1, position: { board: b } });
  const after = state({
    human_player: 1,
    version: 1,
    history: [{ player: 0, dice: [1, 2], notation: 'bar/24 24/22' }],
  });
  expect(soundsForAction(before, after, 'pass')).toEqual([
    'roll',
    'hit',
    'move',
  ]);
  expect(
    soundsForAction(
      before,
      { ...after, history: [{ player: 0, dice: [1, 2], notation: 'pass' }] },
      'pass',
    ),
  ).toEqual(['roll']);
});
it('plays opening/normal human dice once and never rerolls the bot opening on acknowledgment', () => {
  const before = state({ phase: 'opening_roll' });
  const result = state({ phase: 'opening_result', version: 1 });
  expect(soundsForAction(before, result, 'opening_roll')).toEqual(['roll']);
  const b = board();
  b[24] = -2;
  const first = state({ phase: 'opening_result', position: { board: b } });
  expect(
    soundsForAction(
      first,
      state({
        version: 1,
        history: [{ player: 1, dice: [2, 1], notation: '1/off 1/off' }],
      }),
      'start_opening_move',
    ),
  ).toEqual(['bear_off', 'bear_off']);
  expect(
    soundsForAction(state({ phase: 'roll' }), state({ version: 1 }), 'roll'),
  ).toEqual(['roll']);
  expect(
    soundsForAction(result, { ...result, version: 2 }, 'start_opening_move'),
  ).toEqual([]);
});
it('ignores stale states or another match and does not replay existing history', () => {
  const before = state({
    history: [{ player: 1, dice: [2, 1], notation: '1/off' }],
  });
  expect(soundsForAction(before, { ...before, version: 1 }, 'move')).toEqual(
    [],
  );
  expect(
    soundsForAction(before, { ...before, id: 'other', version: 1 }, 'roll'),
  ).toEqual([]);
  expect(soundsForAction(before, before, 'roll')).toEqual([]);
});

it('detects a newly offered bot cube without replaying pending decisions or reloads', async () => {
  const { isNewCubeOffer } = await import('./play-sound-events');
  const before = state();
  const offered = state({
    version: 1,
    phase: 'double_response',
    history: [{ player: 1, dice: [], notation: 'double' }],
  });
  expect(isNewCubeOffer(before, offered)).toBe(true);
  expect(isNewCubeOffer(offered, { ...offered, version: 2 })).toBe(false);
  expect(isNewCubeOffer(before, { ...offered, id: 'other' })).toBe(false);
  expect(isNewCubeOffer(before, { ...offered, version: 0 })).toBe(false);
});
