export function cursorSearchParams(
  limit: number,
  filters: Record<string, string>,
  after?: string,
) {
  const params = new URLSearchParams({ limit: String(limit) });
  for (const [key, value] of Object.entries(filters)) {
    const trimmed = value.trim();
    if (trimmed) params.set(key, trimmed);
  }
  if (after) params.set('after', after);
  return params;
}

export function mergeSearchItems<T>(
  previous: T[],
  incoming: T[],
  itemKey: (item: T) => string,
) {
  const items = new Map(previous.map((item) => [itemKey(item), item]));
  for (const item of incoming) items.set(itemKey(item), item);
  return [...items.values()];
}
