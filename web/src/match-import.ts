/**
 * Client-side selection of match files for upload. The backend validates
 * every file again; this list only keeps unrelated files in a chosen folder
 * from producing an upload and an error row each.
 */
export const MATCH_FILE_EXTENSIONS = [
  '.mat',
  '.txt',
  '.sgf',
  '.xg',
  '.bgf',
  '.ogxm',
] as const;

export const MATCH_FILE_ACCEPT = MATCH_FILE_EXTENSIONS.join(',');

/** Path inside the chosen folder, or the file name for single files. */
export function matchFilePath(file: Pick<File, 'name' | 'webkitRelativePath'>) {
  return file.webkitRelativePath || file.name;
}

function isSupportedMatchFile(path: string) {
  const segments = path.split(/[/\\]/);
  // Hidden files and folders cover macOS metadata such as `._game.mat`
  // AppleDouble files on external volumes and `.Trashes`.
  if (segments.some((segment) => segment.startsWith('.'))) return false;
  const name = segments[segments.length - 1].toLowerCase();
  return MATCH_FILE_EXTENSIONS.some((extension) => name.endsWith(extension));
}

const pathOrder = new Intl.Collator('en', { numeric: true });

/**
 * Supported match files of a folder in a stable, natural path order, and the
 * number of files left out.
 */
export function selectFolderMatchFiles<
  T extends Pick<File, 'name' | 'webkitRelativePath'>,
>(files: Iterable<T>) {
  const all = Array.from(files);
  const supported = all
    .filter((file) => isSupportedMatchFile(matchFilePath(file)))
    .sort((a, b) => pathOrder.compare(matchFilePath(a), matchFilePath(b)));
  return { files: supported, skipped: all.length - supported.length };
}
