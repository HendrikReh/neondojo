import { request, type Match } from '../../shared/api/api';
import type { components } from '../../generated-api';

type Schemas = components['schemas'];
export type CreateSeries = Schemas['CreateSeries'];
export type Series = Schemas['Series'];
export type SeriesDetails = Schemas['SeriesDetails'];
export type SeriesPage = Schemas['SeriesPage'];
export type SideCostSummary = Schemas['SideCostSummary'];
const path = '/engine-matches';

export const listSeries = (signal?: AbortSignal, before?: string) =>
  request<SeriesPage>(
    before ? `${path}?before=${encodeURIComponent(before)}` : path,
    { signal },
  );
export const loadSeries = (id: string, offset: number, signal: AbortSignal) =>
  request<SeriesDetails>(`${path}/${encodeURIComponent(id)}?offset=${offset}`, {
    signal,
  });
export const createSeries = (body: CreateSeries) =>
  request<Series>(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
export const cancelSeries = (id: string) =>
  request<Series>(`${path}/${encodeURIComponent(id)}/cancel`, {
    method: 'POST',
  });
export const seriesSgf = (id: string, index: number) =>
  `/api${path}/${encodeURIComponent(id)}/matches/${index}/sgf`;
/** Library entry of a played match or block; restored under Engine Duels if deleted. */
export const seriesLibraryMatch = (id: string, index: number) =>
  request<Match>(`${path}/${encodeURIComponent(id)}/matches/${index}/library`, {
    method: 'POST',
  });
