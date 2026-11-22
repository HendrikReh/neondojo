import { expect, it } from 'vitest';
import {
  decodeBulkAnalysisSession,
  readSelection,
  saveSelection,
} from './workspace-state';
import { workspaceMessages } from '../shared/i18n/workspace-messages';

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

it('normalizes restored bulk job and database IDs to the API spelling', () => {
  const id = 'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee';
  const databaseId = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa';
  expect(
    decodeBulkAnalysisSession({
      jobs: { [id.toUpperCase()]: databaseId.toUpperCase() },
      showMatchList: true,
    }),
  ).toEqual({ jobs: { [id]: databaseId }, showMatchList: true });
  expect(decodeBulkAnalysisSession({ jobIds: [id.toUpperCase(), id] })).toEqual(
    { jobs: { [id]: null }, showMatchList: false },
  );
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
