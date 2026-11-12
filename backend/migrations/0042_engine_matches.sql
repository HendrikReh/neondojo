CREATE TABLE engine_match_runs (
    id uuid PRIMARY KEY,
    configuration jsonb NOT NULL,
    engines jsonb NOT NULL,
    dice_version text NOT NULL,
    status text NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'completed', 'failed', 'cancelled')),
    total_matches integer NOT NULL CHECK (total_matches BETWEEN 1 AND 2000),
    completed_matches integer NOT NULL DEFAULT 0
        CHECK (completed_matches BETWEEN 0 AND total_matches),
    attempts integer NOT NULL DEFAULT 0,
    lease_token uuid,
    lease_until timestamptz,
    error text,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX engine_match_runs_created ON engine_match_runs(created_at DESC, id DESC);
CREATE INDEX engine_match_runs_queue ON engine_match_runs(created_at, id) WHERE status = 'queued';
CREATE UNIQUE INDEX engine_match_runs_one_worker ON engine_match_runs((true)) WHERE status = 'running';

CREATE TABLE engine_match_results (
    run_id uuid NOT NULL REFERENCES engine_match_runs(id) ON DELETE CASCADE,
    match_index integer NOT NULL CHECK (match_index >= 0),
    seed uuid NOT NULL,
    score_a integer NOT NULL CHECK (score_a >= 0),
    score_b integer NOT NULL CHECK (score_b >= 0 AND score_a <> score_b),
    games integer NOT NULL CHECK (games > 0),
    sgf text NOT NULL CHECK (sgf <> ''),
    PRIMARY KEY (run_id, match_index)
);
