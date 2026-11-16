import { expect, it } from 'vitest';
import { groupCubeActions } from './useGameCubeActions';

const move = (
  move_no: number,
  kind: string,
  player: number,
  notation: string,
  position_uuid: string | null = null,
) =>
  ({
    id: `m${move_no}`,
    match_uuid: 'match',
    game_no: 1,
    move_no,
    kind,
    player,
    notation,
    position_uuid,
    board: [],
    dice: [],
    position_id: '',
    gnubg_match_id: '',
  }) as Parameters<typeof groupCubeActions>[0][number];

it('shows a double and its answer before the next checker move, and a pass at the end', () => {
  const actions = groupCubeActions([
    move(4, 'checker', 1, '24/20', 'p2'),
    move(1, 'checker', 0, '8/5 6/5', 'p1'),
    move(2, 'cube', 1, 'double'),
    move(3, 'cube', 0, 'take'),
    move(5, 'cube', 0, 'double'),
    move(6, 'cube', 1, 'drop'),
  ]);
  expect(actions.before.get('p1')).toBeUndefined();
  expect(actions.before.get('p2')?.map((a) => a.notation)).toEqual([
    'double',
    'take',
  ]);
  expect(actions.trailing.map((a) => [a.player, a.notation])).toEqual([
    [0, 'double'],
    [1, 'drop'],
  ]);
});
