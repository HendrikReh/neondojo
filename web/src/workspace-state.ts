export interface WorkspaceSelection {
  matchId: string;
  gameNo: number;
  positionId?: string;
}
const key = 'bgtutor.workspace';
const uuid = /^[a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12}$/i;
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
