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

export function numberedSearchParams(
  limit: number,
  filters: Record<string, string>,
  page: number,
) {
  const params = cursorSearchParams(limit, filters);
  params.set('page', String(page));
  return params;
}
