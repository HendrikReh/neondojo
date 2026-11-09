/** Older summaries remain usable; only an explicit false denotes a saved snapshot. */
export function hasMatchSource(
  match: { id: string; source_available?: boolean } | null | undefined,
) {
  return match?.source_available !== false;
}
