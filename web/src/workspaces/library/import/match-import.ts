import configurationOptions from '../../../../../shared/configuration-options.json' with { type: 'json' };

/**
 * Client-side selection of match files for upload. The backend validates
 * every file again; this list only keeps unrelated files in a chosen folder
 * from producing an upload and an error row each.
 */
export const MATCH_FILE_EXTENSIONS = configurationOptions.match_file_extensions;

export const MATCH_FILE_ACCEPT = MATCH_FILE_EXTENSIONS.join(',');

/** A file to import and its path inside a chosen or dropped folder. */
export type MatchFileEntry = { file: File; path: string };

/** Path inside the chosen folder, or the file name for single files. */
export function matchFilePath(file: Pick<File, 'name' | 'webkitRelativePath'>) {
  return file.webkitRelativePath || file.name;
}

export function matchFileEntries(files: Iterable<File>): MatchFileEntry[] {
  return Array.from(files, (file) => ({ file, path: matchFilePath(file) }));
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
export function selectFolderMatchFiles(entries: Iterable<MatchFileEntry>) {
  const all = Array.from(entries);
  const supported = all
    .filter((entry) => isSupportedMatchFile(entry.path))
    .sort((a, b) => pathOrder.compare(a.path, b.path));
  return { files: supported, skipped: all.length - supported.length };
}

/** The parts of the File and Directory Entries API a drop needs. */
type DroppedEntry = {
  readonly isFile: boolean;
  readonly isDirectory: boolean;
  readonly name: string;
  readonly fullPath: string;
};
type DroppedFileEntry = DroppedEntry & {
  file(success: (file: File) => void, failure: (error: unknown) => void): void;
};
type DroppedDirectoryEntry = DroppedEntry & {
  createReader(): {
    readEntries(
      success: (entries: DroppedEntry[]) => void,
      failure: (error: unknown) => void,
    ): void;
  };
};
type DroppedItems = {
  readonly length: number;
  [index: number]: {
    readonly kind: string;
    webkitGetAsEntry(): DroppedEntry | null;
  };
};

async function folderEntries(directory: DroppedDirectoryEntry) {
  const reader = directory.createReader();
  const entries: DroppedEntry[] = [];
  // Browsers return a directory in batches; an empty batch ends it.
  for (;;) {
    const batch = await new Promise<DroppedEntry[]>((resolve, reject) =>
      reader.readEntries(resolve, reject),
    );
    if (!batch.length) return entries;
    entries.push(...batch);
  }
}

async function collect(entry: DroppedEntry, files: MatchFileEntry[]) {
  if (entry.isDirectory) {
    for (const child of await folderEntries(entry as DroppedDirectoryEntry))
      await collect(child, files);
  } else if (entry.isFile) {
    const file = await new Promise<File>((resolve, reject) =>
      (entry as DroppedFileEntry).file(resolve, reject),
    );
    files.push({
      file,
      path: entry.fullPath.replace(/^\/+/, '') || entry.name,
    });
  }
}

/**
 * Files of a drop. A drop that contains a folder reads its subfolders too and
 * reports `folder`, so the caller applies the folder rules. The entries are
 * taken synchronously, because a drop's data is only readable while the event
 * is dispatched; call this from the drop handler itself.
 */
export function readDroppedMatchFiles(transfer: {
  readonly items?: DroppedItems | null;
  readonly files: Iterable<File>;
}): Promise<{ files: MatchFileEntry[]; folder: boolean }> {
  const items = transfer.items;
  const roots: DroppedEntry[] = [];
  for (let i = 0; items && i < items.length; i++) {
    const item = items[i];
    const entry = item.kind === 'file' ? item.webkitGetAsEntry?.() : null;
    if (entry) roots.push(entry);
  }
  if (!roots.some((entry) => entry.isDirectory))
    return Promise.resolve({
      files: matchFileEntries(transfer.files),
      folder: false,
    });
  return (async () => {
    const files: MatchFileEntry[] = [];
    for (const root of roots) await collect(root, files);
    return { files, folder: true };
  })();
}
