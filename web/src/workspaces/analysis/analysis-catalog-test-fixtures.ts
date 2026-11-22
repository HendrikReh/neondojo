import type { Job } from '../../shared/api/api';
import type { AnalysisCatalogEntry } from './analysis-catalog-api';
import type { AnalysisCatalog } from './useAnalysisCatalog';
export function entryForJob(job: Job, gameNo?: number): AnalysisCatalogEntry {
  return {
    id: `job:${job.id}`,
    source: 'native',
    scope: gameNo == null ? 'match' : 'game',
    engine: job.engine_snapshot?.engine_version ?? 'GNU Backgammon',
    depth: `${job.plies}-ply`,
    game_no: gameNo ?? null,
    position_id: null,
    created_at: job.created_at ?? '2026-11-15T00:00:00Z',
    status: job.status,
    complete: job.status === 'completed',
    checker_count: 0,
    cube_count: 0,
    job,
  };
}
export function catalogFixture(
  selected?: AnalysisCatalogEntry,
): AnalysisCatalog {
  return {
    sourceRevision: 0,
    entries: selected ? [selected] : [],
    selected,
    selectedId: selected?.id ?? null,
    analysisId: selected?.id,
    loading: false,
    saving: false,
    loadingMore: false,
    nextCursor: null,
    error: undefined,
    select: async () => {},
    remove: async () => false,
    loadMore: async () => {},
    refresh: () => {},
  };
}
