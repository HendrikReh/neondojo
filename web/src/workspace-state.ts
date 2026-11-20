export interface WorkspaceSelection {
  matchId: string;
  gameNo: number;
  positionId?: string;
}
const key = 'neondojo.workspace';
const uuid = /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i;

export type BulkAnalysisSession = {
  jobs: Record<string, string | null>;
  showMatchList: boolean;
};

export function decodeBulkAnalysisSession(
  value: unknown,
): BulkAnalysisSession | undefined {
  if (!value || typeof value !== 'object') return;
  const jobs: BulkAnalysisSession['jobs'] = {};
  if ('jobs' in value) {
    if (
      !value.jobs ||
      typeof value.jobs !== 'object' ||
      Array.isArray(value.jobs)
    )
      return;
    for (const [id, databaseId] of Object.entries(value.jobs)) {
      if (
        !uuid.test(id) ||
        (databaseId !== null &&
          (typeof databaseId !== 'string' || !uuid.test(databaseId)))
      )
        return;
      jobs[id] = databaseId;
    }
  } else {
    if (!('jobIds' in value) || !Array.isArray(value.jobIds)) return;
    for (const id of value.jobIds) {
      if (typeof id !== 'string' || !uuid.test(id)) return;
      jobs[id] = null;
    }
  }
  return {
    jobs,
    showMatchList: 'showMatchList' in value && value.showMatchList === true,
  };
}

export function readSelection(
  storage?: Pick<Storage, 'getItem'>,
): WorkspaceSelection | null {
  try {
    const value = JSON.parse((storage ?? localStorage).getItem(key) ?? 'null');
    if (
      !value ||
      typeof value.matchId !== 'string' ||
      !uuid.test(value.matchId) ||
      !Number.isSafeInteger(value.gameNo) ||
      value.gameNo < 1 ||
      (value.positionId !== undefined &&
        (typeof value.positionId !== 'string' || !uuid.test(value.positionId)))
    )
      return null;
    return {
      matchId: value.matchId,
      gameNo: value.gameNo,
      ...(value.positionId ? { positionId: value.positionId } : {}),
    };
  } catch {
    return null;
  }
}
export function saveSelection(
  value: WorkspaceSelection,
  storage?: Pick<Storage, 'setItem'>,
) {
  try {
    (storage ?? localStorage).setItem(key, JSON.stringify(value));
  } catch {
    /* Navigation works without persistent storage. */
  }
}
