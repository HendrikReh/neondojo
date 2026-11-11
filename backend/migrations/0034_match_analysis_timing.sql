-- Confirmed timing belongs to a job and is committed with its game checkpoint.
CREATE TABLE match_analysis_timing (
    job_id uuid PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
    total_moves bigint NOT NULL CHECK (total_moves >= 0),
    completed_moves bigint NOT NULL CHECK (completed_moves >= 0 AND completed_moves <= total_moves),
    timed_moves bigint NOT NULL DEFAULT 0 CHECK (timed_moves >= 0 AND timed_moves <= completed_moves),
    measured_elapsed_ms double precision NOT NULL DEFAULT 0 CHECK (measured_elapsed_ms >= 0 AND measured_elapsed_ms < 'Infinity'::float8),
    benchmark_id uuid REFERENCES benchmark_runs(id) ON DELETE SET NULL,
    benchmark_average_move_ms double precision CHECK (benchmark_average_move_ms > 0 AND benchmark_average_move_ms < 'Infinity'::float8),
    lease_token uuid NOT NULL,
    attempt integer NOT NULL,
    active_elapsed_ms double precision NOT NULL DEFAULT 0 CHECK (active_elapsed_ms >= 0 AND active_elapsed_ms < 'Infinity'::float8),
    active_started_at timestamptz
);
