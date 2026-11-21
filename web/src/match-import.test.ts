import { expect, it } from 'vitest';
import {
  MATCH_FILE_ACCEPT,
  matchFileEntries,
  matchFilePath,
  readDroppedMatchFiles,
  selectFolderMatchFiles,
} from './match-import';

const entry = (path: string) => ({
  file: new File(['x'], path.split('/').pop() ?? path),
  path,
});
const paths = (entries: readonly { path: string }[]) =>
  entries.map((e) => e.path);

it('keeps supported match formats of a folder in natural path order', () => {
  const { files, skipped } = selectFolderMatchFiles([
    entry('Matches/Round 10.mat'),
    entry('Matches/notes.pdf'),
    entry('Matches/Round 2.SGF'),
    entry('Matches/2024/final.xg'),
    entry('Matches/transcript.txt'),
    entry('Matches/tutorial.bgf'),
    entry('Matches/hedgehog.ogxm'),
    entry('Matches/archive.xgp'),
    entry('Matches/session.ogxms'),
  ]);
  expect(paths(files)).toEqual([
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
    entry('Matches/._game.mat'),
    entry('Matches/.Trashes/old.sgf'),
    entry('Matches/.DS_Store'),
    entry('Matches/game.mat'),
  ]);
  expect(paths(files)).toEqual(['Matches/game.mat']);
  expect(skipped).toBe(3);
});

it('names picked files by their folder path, or by their name alone', () => {
  expect(matchFilePath({ name: 'game.sgf', webkitRelativePath: '' })).toBe(
    'game.sgf',
  );
  const picked = Object.defineProperty(
    new File(['x'], 'game.sgf'),
    'webkitRelativePath',
    { value: 'Matches/game.sgf' },
  );
  expect(paths(matchFileEntries([picked]))).toEqual(['Matches/game.sgf']);
});

it('offers the same formats to the file picker', () => {
  expect(MATCH_FILE_ACCEPT).toBe('.mat,.txt,.sgf,.xg,.bgf,.ogxm');
});

// Minimal stand-ins for the browser's File and Directory Entries API.
type FakeEntry = {
  isFile: boolean;
  isDirectory: boolean;
  name: string;
  fullPath: string;
  file?: (ok: (file: File) => void, fail: (e: unknown) => void) => void;
  createReader?: () => {
    readEntries: (
      ok: (e: FakeEntry[]) => void,
      fail: (e: unknown) => void,
    ) => void;
  };
};
const fileEntry = (fullPath: string): FakeEntry => {
  const name = fullPath.split('/').pop() ?? fullPath;
  return {
    isFile: true,
    isDirectory: false,
    name,
    fullPath,
    file: (ok) => ok(new File([fullPath], name)),
  };
};
/** A folder whose reader returns its children in batches of `batch`. */
const folderEntry = (
  fullPath: string,
  children: FakeEntry[],
  batch = 2,
): FakeEntry => ({
  isFile: false,
  isDirectory: true,
  name: fullPath.split('/').pop() ?? fullPath,
  fullPath,
  createReader: () => {
    let next = 0;
    return {
      readEntries: (ok) => {
        ok(children.slice(next, next + batch));
        next += batch;
      },
    };
  },
});
const drop = (entries: FakeEntry[], files: File[] = []) => ({
  items: Object.assign(
    entries.map((e) => ({ kind: 'file', webkitGetAsEntry: () => e })),
    { length: entries.length },
  ),
  files,
});

it('reads a dropped folder with its subfolders across reader batches', async () => {
  const dropped = await readDroppedMatchFiles(
    drop([
      folderEntry('/Matches', [
        fileEntry('/Matches/a.mat'),
        fileEntry('/Matches/b.sgf'),
        fileEntry('/Matches/notes.pdf'),
        folderEntry('/Matches/2014', [fileEntry('/Matches/2014/final.xg')]),
        folderEntry('/Matches/empty', []),
      ]),
      fileEntry('/loose.bgf'),
    ]),
  );
  expect(dropped.folder).toBe(true);
  expect(paths(dropped.files)).toEqual([
    'Matches/a.mat',
    'Matches/b.sgf',
    'Matches/notes.pdf',
    'Matches/2014/final.xg',
    'loose.bgf',
  ]);
  expect(await dropped.files[3].file.text()).toBe('/Matches/2014/final.xg');
});

it('keeps a drop of plain files a file import', async () => {
  const files = [new File(['x'], 'game.sgf'), new File(['y'], 'notes.pdf')];
  const dropped = await readDroppedMatchFiles(
    drop([fileEntry('/game.sgf'), fileEntry('/notes.pdf')], files),
  );
  expect(dropped.folder).toBe(false);
  expect(dropped.files.map((e) => e.file)).toEqual(files);
  expect(paths(dropped.files)).toEqual(['game.sgf', 'notes.pdf']);
});

it('falls back to the dropped files without the entries API', async () => {
  const files = [new File(['x'], 'game.sgf')];
  const dropped = await readDroppedMatchFiles({ files });
  expect(dropped).toEqual({
    files: [{ file: files[0], path: 'game.sgf' }],
    folder: false,
  });
});

it('reports a folder that cannot be read', async () => {
  const broken: FakeEntry = {
    ...folderEntry('/Locked', []),
    createReader: () => ({
      readEntries: (_ok, fail) => fail(new Error('NotReadableError')),
    }),
  };
  await expect(readDroppedMatchFiles(drop([broken]))).rejects.toThrow(
    'NotReadableError',
  );
});
