import { expect, it } from 'vitest';
import { readSelection, saveSelection } from './workspace-state';
import { workspaceMessages } from './workspace-messages';

it('restores only validated match/game/position selections and tolerates blocked storage', () => {
  const matchId = '00000000-0000-0000-0000-000000000001',
    positionId = '00000000-0000-0000-0000-000000000002';
  expect(
    readSelection({
      getItem: () => JSON.stringify({ matchId, gameNo: 2, positionId }),
    }),
  ).toEqual({ matchId, gameNo: 2, positionId });
  for (const value of [
    '{',
    '{}',
    JSON.stringify({ matchId, gameNo: -1 }),
    JSON.stringify({ matchId: 'invalid', gameNo: 1 }),
  ])
    expect(readSelection({ getItem: () => value })).toBeNull();
  expect(
    readSelection({
      getItem: () => {
        throw new Error('denied');
      },
    }),
  ).toBeNull();
  expect(() =>
    saveSelection(
      { matchId, gameNo: 1 },
      {
        setItem: () => {
          throw new Error('denied');
        },
      },
    ),
  ).not.toThrow();
});
it('keeps workspace translations and dynamic arguments complete', () => {
  const source = workspaceMessages.en;
  for (const locale of Object.values(workspaceMessages)) {
    expect(Object.keys(locale).sort()).toEqual(Object.keys(source).sort());
    for (const key of Object.keys(source) as Array<keyof typeof source>) {
      const args = (s: string) =>
        [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
      expect(args(locale[key])).toEqual(args(source[key]));
    }
  }
});
