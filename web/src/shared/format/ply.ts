/** Consistent ply depth label across workspaces (e.g. "2-ply"). */
export function formatPly(
  plies: number,
  formatNumber?: (value: number) => string,
): string {
  return `${formatNumber ? formatNumber(plies) : plies}-ply`;
}
