import type { components } from './generated-api';

type Schema = components['schemas'];
export type AnalysisCatalogEntry = Schema['AnalysisCatalogEntry'];
export type AnalysisCatalogPage = Schema['AnalysisCatalogPage'];
export type PositionAnalysisResult = Schema['SelectedPositionAnalysis'];
export type MatchAnalysisStatistics = Schema['MatchAnalysisStatistics'];

export function analysisCatalogPath(
  matchId: string,
  positionId?: string,
  cursor?: string,
) {
  const query = new URLSearchParams({ limit: '50' });
  if (positionId) query.set('position_id', positionId);
  if (cursor) query.set('cursor', cursor);
  return `/matches/${encodeURIComponent(matchId)}/analyses?${query}`;
}

export function analysisResultPath(positionId: string, analysisId: string) {
  const query = new URLSearchParams({ analysis_id: analysisId });
  return `/positions/${encodeURIComponent(positionId)}/analysis-result?${query}`;
}

export function analysisStatisticsPath(matchId: string, analysisId: string) {
  const query = new URLSearchParams({ analysis_id: analysisId });
  return `/matches/${encodeURIComponent(matchId)}/analysis-statistics?${query}`;
}

export function mergeCatalogEntries(
  existing: AnalysisCatalogEntry[],
  incoming: AnalysisCatalogEntry[],
) {
  return [
    ...new Map(
      [...existing, ...incoming].map((entry) => [entry.id, entry]),
    ).values(),
  ];
}
