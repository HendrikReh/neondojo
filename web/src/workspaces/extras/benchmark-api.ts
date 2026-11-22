import { request } from '../../shared/api/api';

import type { components, paths } from '../../generated-api';

type Schemas = components['schemas'];
export type BenchmarkReference = Schemas['BenchmarkReference'];
export type BenchmarkRun = Schemas['BenchmarkRun'];
type BenchmarkRequest =
  paths['/api/benchmarks']['post']['requestBody']['content']['application/json'];

export const loadBenchmarkReference = (signal?: AbortSignal) =>
  request<BenchmarkReference>('/benchmarks/reference', { signal });
export const loadBenchmarks = (signal?: AbortSignal) =>
  request<Schemas['BenchmarkHistory']>('/benchmarks', { signal });
export const startBenchmarks = (engine_id: string, plies: number[]) => {
  const body: BenchmarkRequest = { engine_id, plies };
  return request<BenchmarkRun[]>('/benchmarks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
};
export const startPlayBenchmark = (engine_id?: string, plies?: number) =>
  request<BenchmarkRun>('/benchmarks/play', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body:
      engine_id === undefined
        ? undefined
        : JSON.stringify({ engine_id, plies }),
  });
export const cancelBenchmark = (id: string) =>
  request<BenchmarkRun>(`/benchmarks/${encodeURIComponent(id)}/cancel`, {
    method: 'POST',
  });
export const deleteBenchmark = (id: string) =>
  request<void>(`/benchmarks/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  });
export const loadBenchmarkStatus = (signal?: AbortSignal) =>
  request<Schemas['BenchmarkStatus']>('/benchmarks/status', { signal });
