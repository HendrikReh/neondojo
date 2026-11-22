import { expect, it } from 'vitest';
import type { PlaySession } from '../../workspaces/play/play-api';
import { actionFrames, checkerFrame, reverseFrame } from './play-animation';
const board = () => Array<number>(26).fill(0);
function session(values: Partial<PlaySession> = {}): PlaySession {
  return {
    id: 'match',
    version: 0,
    human_player: 0,
    human_name: 'Player',
    match_length: 7,
    bearoff_direction: 'right',
    engine: { id: 'gnubg', version: '1.08.003', plies: 2 },
    position: {
      id: 'match',
      match_uuid: 'match',
      ordinal: 0,
      game_no: 1,
      ply: 0,
      player: 0,
      dice: [],
      board: board(),
      position_id: '',
      gnubg_match_id: '',
      notation: '',
    },
    phase: 'move',
    can_double: false,
    legal_moves: [],
    history: [],
    scores: [0, 0],
    game_no: 1,
    winner: null,
    opening: null,
    ...values,
  };
}
it('shows opening bot dice without a new roll, then replays the first bot moves before yielding', () => {
  const before = session({ phase: 'opening_result' });
  before.position.board[1] = -2;
  before.opening = { white_die: 1, black_die: 5, winner: 1, rolls: [[1, 5]] };
  const after = session({
    version: 1,
    phase: 'roll',
    history: [{ player: 1, dice: [5, 1], notation: '24/19 24/23' }],
  });
  const frames = actionFrames(before, after, 'start_opening_move');
  expect(frames.map((f) => f.kind)).toEqual(['dice', 'checker', 'checker']);
  expect(frames[0]).toMatchObject({ dice: [5, 1], player: 1, roll: false });
  if (frames[1].kind !== 'checker' || frames[2].kind !== 'checker')
    throw new Error('Expected checker frames');
  expect(frames[1].motions[0]).toEqual({ from: 1, to: 6, player: 1 });
  expect(frames[2].after[2]).toBe(-1);
});
it('silently applies previously previewed human moves before bot roll/hits', () => {
  const before = session();
  before.position.board[6] = 2;
  before.position.board[1] = -1;
  const after = session({
    version: 1,
    history: [
      { player: 0, dice: [2, 1], notation: '6/4 6/5' },
      { player: 1, dice: [3, 1], notation: '24/21 21/20' },
    ],
  });
  const frames = actionFrames(before, after, 'move');
  expect(frames.map((f) => f.kind)).toEqual(['dice', 'checker', 'checker']);
  expect(frames[0].before[4]).toBe(1);
  expect(frames[0]).toMatchObject({ roll: true, dice: [3, 1], player: 1 });
  if (frames[1].kind !== 'checker' || frames[2].kind !== 'checker')
    throw new Error('Expected checker frames');
  expect(frames[1].sound).toBe('hit');
  expect(frames[1].motions).toEqual([
    { from: 1, to: 4, player: 1 },
    { from: 4, to: 25, player: 0 },
  ]);
  expect(frames[2].after[25]).toBe(2);
});
it('animates each native opening attempt, preserving per-side dice and ties', () => {
  const before = session({ phase: 'opening_roll' }),
    after = session({
      version: 1,
      phase: 'opening_result',
      opening: {
        white_die: 2,
        black_die: 1,
        winner: 0,
        rolls: [
          [3, 3],
          [6, 6],
          [2, 1],
        ],
      },
    });
  const frames = actionFrames(before, after, 'opening_roll');
  expect(frames.map((f) => f.dice)).toEqual([
    [3, 3],
    [6, 6],
    [2, 1],
  ]);
  expect(frames.every((f) => f.kind === 'dice' && f.opening && f.roll)).toBe(
    true,
  );
  expect(frames.map((f) => f.kind === 'dice' && f.tied)).toEqual([
    true,
    true,
    false,
  ]);
});
it('handles both colors, bar entry, bearing off and reversed hit restoration', () => {
  for (const player of [0, 1]) {
    const b = board(),
      from = player === 0 ? 25 : 0,
      to = player === 0 ? 24 : 1;
    b[from] = player === 0 ? 1 : -1;
    b[to] = player === 0 ? -1 : 1;
    const frame = checkerFrame(b, { from, to }, player, [1, 2]);
    expect(frame.motions.length).toBe(2);
    const reverse = reverseFrame(frame);
    expect(reverse.before).toEqual(frame.after);
    expect(reverse.after).toEqual(b);
    expect(reverse.motions[0]).toEqual({ from: to, to: from, player });
    const off = checkerFrame(
      frame.after,
      { from: to, to: player === 0 ? -1 : 26 },
      player,
      [6, 2],
    );
    expect(off.sound).toBe('bear_off');
    expect(off.after[to]).toBe(0);
  }
});
it('animates human rolls, ignores stale states and offers no checker animation for cube/pass', () => {
  const before = session({ phase: 'roll' }),
    after = session({ version: 1 });
  after.position.dice = [3, 1];
  expect(actionFrames(before, after, 'roll')[0]).toMatchObject({
    kind: 'dice',
    roll: true,
    dice: [3, 1],
  });
  expect(actionFrames(before, before, 'roll')).toEqual([]);
  expect(actionFrames(before, { ...after, id: 'other' }, 'roll')).toEqual([]);
  expect(actionFrames(before, after, 'double')).toEqual([]);
});

it('provides finite, in-board motion endpoints for every theme, bar and off tray', async () => {
  const { checkerCenter } = await import('./PlayCheckerMotion');
  const { archiveThemes } = await import('./themes');
  const b = board();
  b[6] = 15;
  b[19] = -15;
  for (const theme of [undefined, ...archiveThemes])
    for (const player of [0, 1])
      for (const point of [-1, 0, 1, 6, 12, 13, 19, 24, 25, 26]) {
        const center = checkerCenter(b, point, player, theme);
        expect(Number.isFinite(center.x) && Number.isFinite(center.y)).toBe(
          true,
        );
        expect(center.x).toBeGreaterThanOrEqual(0);
        expect(center.x).toBeLessThan(theme?.background.width ?? 730);
        expect(center.y).toBeGreaterThanOrEqual(0);
        expect(center.y).toBeLessThan(theme?.background.height ?? 470);
      }
});
