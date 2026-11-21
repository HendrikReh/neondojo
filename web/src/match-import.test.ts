import { expect, it } from 'vitest';
import {
  MATCH_FILE_ACCEPT,
  matchFilePath,
  selectFolderMatchFiles,
} from './match-import';

const file = (webkitRelativePath: string) => ({
  name: webkitRelativePath.split('/').pop() ?? webkitRelativePath,
  webkitRelativePath,
});

it('keeps supported match formats of a folder in natural path order', () => {
  const { files, skipped } = selectFolderMatchFiles([
    file('Matches/Round 10.mat'),
    file('Matches/notes.pdf'),
    file('Matches/Round 2.SGF'),
    file('Matches/2024/final.xg'),
    file('Matches/transcript.txt'),
    file('Matches/tutorial.bgf'),
    file('Matches/hedgehog.ogxm'),
    file('Matches/archive.xgp'),
    file('Matches/session.ogxms'),
  ]);
  expect(files.map(matchFilePath)).toEqual([
    'Matches/2024/final.xg',
    'Matches/hedgehog.ogxm',
    'Matches/Round 2.SGF',
    'Matches/Round 10.mat',
    'Matches/transcript.txt',
    'Matches/tutorial.bgf',
  ]);
  expect(skipped).toBe(3);
});

it('skips hidden files and hidden folders such as macOS metadata', () => {
  const { files, skipped } = selectFolderMatchFiles([
    file('Matches/._game.mat'),
    file('Matches/.Trashes/old.sgf'),
    file('Matches/.DS_Store'),
    file('Matches/game.mat'),
  ]);
  expect(files.map(matchFilePath)).toEqual(['Matches/game.mat']);
  expect(skipped).toBe(3);
});

it('names single files without a folder path by their file name', () => {
  expect(matchFilePath({ name: 'game.sgf', webkitRelativePath: '' })).toBe(
    'game.sgf',
  );
});

it('offers the same formats to the file picker', () => {
  expect(MATCH_FILE_ACCEPT).toBe('.mat,.txt,.sgf,.xg,.bgf,.ogxm');
});
