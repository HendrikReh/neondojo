type ProvenanceText = (
  key: 'nativeSource' | 'xgSource' | 'bgblitzSource' | 'unknownDepth',
) => string;

/** Imported search/rollout labels are not interchangeable with GNUbg ply counts. */
export function mistakeProvenance(
  value: {
    engine: string;
    plies?: number | null;
    depth?: string | null;
    method?: string | null;
    source?: 'native' | 'xg' | 'bgblitz' | null;
  },
  text: ProvenanceText,
): string {
  const source = value.source
    ? text(
        value.source === 'native'
          ? 'nativeSource'
          : value.source === 'xg'
            ? 'xgSource'
            : 'bgblitzSource',
      )
    : undefined;
  const depth =
    value.depth?.trim() ||
    (value.plies == null ? text('unknownDepth') : `${value.plies} ply`);
  return [source, value.engine, depth, value.method?.trim()]
    .filter(Boolean)
    .join(' · ');
}

export function mistakeReferenceKey(reference: {
  job_id?: string | null;
  imported_analysis_id?: string | null;
  game_no: number;
  decision_no: number;
}): string {
  const source = reference.job_id
    ? `native:${reference.job_id}`
    : `imported:${reference.imported_analysis_id}`;
  return `${source}:${reference.game_no}:${reference.decision_no}`;
}
