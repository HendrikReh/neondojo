import { request } from './api';

export interface BenchmarkProtocol {
  id: string;
  game_timeout_seconds: number | null;
  [key: string]: unknown;
}
export interface BenchmarkReference {
  id: string;
  version: number;
  name: string;
  sha256: string;
  match_length: number;
  total_games: number;
  total_moves: number;
  protocol: BenchmarkProtocol;
}
export interface BenchmarkRun {
  id: string;
  status: 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';
  created_at: string;
  started_at: string | null;
  finished_at: string | null;
  machine: {
    id: string;
    name: string;
    os: string;
    architecture: string;
    cpu_model: string | null;
    logical_cpus: number | null;
    memory_bytes: number | null;
  };
  engine_id: string;
  engine_name: string;
  engine_version: string;
  engine_path: string;
  plies: number;
  reference: BenchmarkReference;
  protocol: BenchmarkProtocol;
  total_elapsed_ms: number | null;
  average_move_ms: number | null;
  completed_games: number;
  analyzed_moves: number;
  error: string | null;
}
export const loadBenchmarkReference = (signal?: AbortSignal) =>
  request<BenchmarkReference>('/benchmarks/reference', { signal });
export const loadBenchmarks = (signal?: AbortSignal) =>
  request<{ runs: BenchmarkRun[]; active: boolean }>('/benchmarks', { signal });
export const startBenchmarks = (engine_id: string, plies: number[]) =>
  request<BenchmarkRun[]>('/benchmarks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ engine_id, plies }),
  });
export const cancelBenchmark = (id: string) =>
  request<BenchmarkRun>(`/benchmarks/${encodeURIComponent(id)}/cancel`, {
    method: 'POST',
  });
export const loadBenchmarkStatus = (signal?: AbortSignal) =>
  request<{ active: boolean }>('/benchmarks/status', { signal });
