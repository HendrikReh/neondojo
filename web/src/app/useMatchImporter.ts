import { useEffect, useRef, useState } from 'react';
import { request, type Match } from '../shared/http/api';
import { AppError } from '../shared/i18n/i18n-core';
import type {
  ImportResult,
  ImportRun,
} from '../workspaces/library/import/ImportRunPanel';
import {
  readDroppedMatchFiles,
  selectFolderMatchFiles,
  type MatchFileEntry,
} from '../workspaces/library/import/match-import';
import { acceptsLibraryMatches } from '../workspaces/library/library-api';
import type { ImportWarning } from '../workspaces/library/library-messages';

type ImportSource = {
  files: readonly MatchFileEntry[];
  /** Set for a folder, with the number of files left out. */
  folder?: { skipped: number };
};

export type MatchImporter = ReturnType<typeof useMatchImporter>;

/** Imports match files, folders and drops, one import at a time. */
export function useMatchImporter({
  databaseId,
  intent,
  onStart,
  onOpen,
  onChanged,
  onBatchFinished,
  onError,
}: {
  /** The destination unless the import names its own. */
  databaseId: string;
  /** A later navigation keeps an import from changing the view. */
  intent: () => number;
  onStart: () => void;
  /** Opens the match of a single import. */
  onOpen: (match: Match) => void;
  /** The library has new matches. */
  onChanged: () => void;
  onBatchFinished: () => void;
  onError: (error: unknown) => void;
}) {
  const [importRun, setImportRun] = useState<ImportRun>();
  const importStop = useRef(false);
  const importBusy = useRef(false);
  const [notice, setNotice] = useState<{
    duplicate: boolean;
    count: number;
    warnings: ImportWarning[];
  }>();
  const [uploading, setUploading] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const folderInput = useRef<HTMLInputElement>(null);
  const importDestination = useRef<string | undefined>(undefined);
  useEffect(() => {
    const chooser = input.current;
    const clearDestination = () => {
      importDestination.current = undefined;
    };
    const folderChooser = folderInput.current;
    // React has no typed prop for the directory picker attribute.
    if (folderChooser) folderChooser.webkitdirectory = true;
    chooser?.addEventListener('cancel', clearDestination);
    folderChooser?.addEventListener('cancel', clearDestination);
    return () => {
      chooser?.removeEventListener('cancel', clearDestination);
      folderChooser?.removeEventListener('cancel', clearDestination);
    };
  }, []);
  function chooseImportFiles(destination?: string) {
    importDestination.current = destination;
    input.current?.click();
  }
  function chooseImportFolder(destination: string) {
    importDestination.current = destination;
    folderInput.current?.click();
  }
  /**
   * Imports files one by one. `source` may still be reading a dropped folder;
   * the import counts as running from the call on, so a second drop or pick
   * waits for this one.
   */
  async function upload(
    source: ImportSource | Promise<ImportSource>,
    destination = databaseId,
  ) {
    if (importBusy.current) return;
    importBusy.current = true;
    setUploading(true);
    const startIntent = intent();
    try {
      let files: readonly MatchFileEntry[];
      let folder: ImportSource['folder'];
      try {
        ({ files, folder } = await source);
      } catch (e) {
        onError(e);
        return;
      }
      await importFiles(files, destination, folder, startIntent);
    } finally {
      importBusy.current = false;
      setUploading(false);
      if (input.current) input.current.value = '';
    }
  }
  async function importFiles(
    selected: readonly MatchFileEntry[],
    destination: string,
    folder: ImportSource['folder'],
    startIntent: number,
  ) {
    if (!selected.length && !folder) return;
    onStart();
    onError(undefined);
    setNotice(undefined);
    importStop.current = false;
    // A folder run always reports in the results panel, even for one file.
    const batch = !!folder || selected.length > 1;
    const run: ImportRun = {
      results: [],
      total: selected.length,
      skipped: folder?.skipped ?? 0,
      folder: !!folder,
      running: selected.length > 0,
      stopping: false,
      stopped: false,
    };
    setImportRun(batch ? run : undefined);
    if (!selected.length) return;
    const target =
      destination && acceptsLibraryMatches(destination)
        ? `?database_id=${encodeURIComponent(destination)}`
        : '';
    const results: ImportResult[] = [];
    // Large batches refresh the library at most every two seconds instead of
    // reloading the list after every file.
    let refreshedAt = 0;
    let unrefreshed = false;
    for (const { file, path: name } of selected) {
      if (importStop.current) break;
      try {
        if (file.size > 16 * 1024 * 1024) throw new AppError('error.fileSize');
        const body = new FormData();
        body.append('file', file);
        const imported = await request<{
          match: Match;
          duplicate: boolean;
          warnings: ImportWarning[];
        }>(`/imports${target}`, { method: 'POST', body });
        unrefreshed = true;
        if (!batch || Date.now() - refreshedAt >= 2000) {
          onChanged();
          refreshedAt = Date.now();
          unrefreshed = false;
        }
        results.push({
          name,
          status: imported.duplicate ? 'duplicate' : 'importedFile',
          warnings: imported.warnings,
        });
        if (!batch && intent() === startIntent) {
          onOpen(imported.match);
          setNotice({
            duplicate: imported.duplicate,
            count: imported.match.position_count,
            warnings: imported.warnings,
          });
        }
      } catch (e) {
        results.push({ name, status: 'error', detail: e });
        if (!batch) onError(e);
      }
      if (batch)
        setImportRun(
          (current) => current && { ...current, results: [...results] },
        );
    }
    if (unrefreshed) onChanged();
    if (batch) {
      const stopped = results.length < selected.length;
      setImportRun(
        (current) =>
          current && {
            ...current,
            results: [...results],
            running: false,
            stopping: false,
            stopped,
          },
      );
      if (intent() === startIntent) onBatchFinished();
    }
  }
  /** Reads a drop now, while its data is still available. */
  function dropImport(transfer: DataTransfer) {
    if (importBusy.current) return;
    void upload(
      readDroppedMatchFiles(transfer).then(({ files, folder }) => {
        if (!folder) return { files };
        const selected = selectFolderMatchFiles(files);
        return {
          files: selected.files,
          folder: { skipped: selected.skipped },
        };
      }),
    );
  }
  function stopImport() {
    importStop.current = true;
    setImportRun((current) => current && { ...current, stopping: true });
  }
  /** The destination of the open file chooser; it applies once. */
  function takeDestination() {
    const destination = importDestination.current;
    importDestination.current = undefined;
    return destination;
  }
  return {
    uploading,
    run: importRun,
    stop: stopImport,
    closeRun: () => setImportRun(undefined),
    notice,
    closeNotice: () => setNotice(undefined),
    chooseFiles: chooseImportFiles,
    chooseFolder: chooseImportFolder,
    drop: dropImport,
    upload,
    takeDestination,
    input,
    folderInput,
  };
}
