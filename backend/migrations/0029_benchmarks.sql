CREATE TABLE benchmark_machine (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    id uuid NOT NULL UNIQUE
);
CREATE TABLE benchmark_runs (
    id uuid PRIMARY KEY,
    queue_no bigserial NOT NULL UNIQUE,
    status text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued','running','completed','failed','cancelled')),
    created_at timestamptz NOT NULL DEFAULT now(),
    started_at timestamptz,
    finished_at timestamptz,
    machine jsonb NOT NULL,
    engine_id text NOT NULL,
    engine_name text NOT NULL,
    engine_version text NOT NULL,
    engine_path text NOT NULL,
    plies integer NOT NULL CHECK (plies BETWEEN 0 AND 4),
    reference jsonb NOT NULL,
    protocol jsonb NOT NULL,
    reference_sgf text NOT NULL,
    total_elapsed_ms double precision,
    average_move_ms double precision,
    completed_games integer NOT NULL DEFAULT 0 CHECK (completed_games BETWEEN 0 AND 10),
    analyzed_moves integer NOT NULL DEFAULT 0 CHECK (analyzed_moves BETWEEN 0 AND 460),
    error text,
    worker_token uuid,
    lease_until timestamptz,
    CHECK ((status='completed' AND total_elapsed_ms > 0 AND total_elapsed_ms < 'Infinity'::float8
        AND average_move_ms > 0 AND average_move_ms < 'Infinity'::float8
        AND completed_games=10 AND analyzed_moves=460 AND finished_at IS NOT NULL)
        OR (status <> 'completed' AND total_elapsed_ms IS NULL AND average_move_ms IS NULL))
);
CREATE INDEX benchmark_runs_history ON benchmark_runs(created_at DESC, queue_no DESC);
CREATE INDEX benchmark_runs_queue ON benchmark_runs(created_at, queue_no) WHERE status='queued';
CREATE UNIQUE INDEX benchmark_one_running ON benchmark_runs((true)) WHERE status='running';
