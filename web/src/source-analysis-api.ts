import { request } from './api';
import type { components } from './generated-api';
type Schema = components['schemas'];
export type SourceCandidate = Schema['ImportedCandidate'];
export type SourceAnalysis = Schema['ImportedAnalysisRecord'];
export type BookConfig = Schema['BookConfig'];
export type BookStatus = Schema['BookStatus'];
export type BookSettings = Schema['BookSettings'];
export type BookLookup = Schema['BookLookup'];
export type XgBookStatus = Schema['XgBookStatus'];
export type XgPositionResult = Schema['XgPositionResult'];
export const bookSettings = (signal: AbortSignal) =>
  request<BookSettings>('/opening-book', { signal });
export const sourceAnalyses = (positionId: string, signal: AbortSignal) =>
  request<SourceAnalysis[]>(`/positions/${positionId}/imported-analysis`, {
    signal,
  });
export const bookLookup = (positionId: string, signal: AbortSignal) =>
  request<BookLookup>(`/positions/${positionId}/opening-book`, { signal });
