import { useEffect, useState } from 'react';
import {
  libraryRowKey,
  type ActionResponse,
  type LibraryMatch,
} from './library-api';

const succeededBatchStatuses = new Set([
  'copied',
  'moved',
  'deleted',
  'tagged',
  'same_database',
  'duplicate',
]);

export function useLibrarySelection({
  matchScope,
  matches,
  onMutated,
}: {
  matchScope: string;
  matches: LibraryMatch[];
  onMutated: (deletedIds: string[]) => void;
}) {
  const [selectedRows, setSelectedRows] = useState<LibraryMatch[]>([]);
  const [result, setResult] = useState<ActionResponse>();
  const [resultNames, setResultNames] = useState<Record<string, string>>({});

  useEffect(() => {
    setSelectedRows([]);
    setResult(undefined);
  }, [matchScope]);

  const selected = new Set(selectedRows.map((row) => row.id));
  const visibleRows = new Map(
    matches.map((match) => [libraryRowKey(match), match]),
  );
  const selection = selectedRows.map(
    (match) => visibleRows.get(libraryRowKey(match)) ?? match,
  );

  function changeSelection(rows: readonly LibraryMatch[], checked: boolean) {
    const next = new Map(selectedRows.map((row) => [row.id, row]));
    for (const row of rows) {
      if (!checked) next.delete(row.id);
      else if (next.size < 100) next.set(row.id, row);
    }
    setSelectedRows([...next.values()]);
  }

  function clearSelection() {
    setSelectedRows([]);
  }

  function resetTransientState() {
    setSelectedRows([]);
    setResult(undefined);
  }

  function actionResult(response: ActionResponse) {
    setResult(response);
    setResultNames(
      Object.fromEntries(selectedRows.map((m) => [m.id, m.display_name])),
    );
    const deleted = response.results
      .filter((r) => r.status === 'deleted')
      .map((r) => r.match_id);
    const failed = new Set(
      response.results
        .filter((row) => !succeededBatchStatuses.has(row.status))
        .map((row) => row.match_id),
    );
    setSelectedRows((rows) => rows.filter((row) => failed.has(row.id)));
    onMutated(deleted);
  }

  return {
    selected,
    selection,
    result,
    resultNames,
    changeSelection,
    clearSelection,
    resetTransientState,
    actionResult,
  };
}
