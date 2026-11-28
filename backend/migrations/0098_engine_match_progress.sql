-- Lease-scoped display state. Archived results and resume checkpoints stay in
-- engine_match_results and engine_match_runs.
CREATE TABLE engine_match_progress (
    run_id uuid PRIMARY KEY REFERENCES engine_match_runs(id) ON DELETE CASCADE,
    lease_token uuid NOT NULL,
    unit_number integer NOT NULL,
    game_number integer NOT NULL,
    completed_games integer NOT NULL,
    score_a integer NOT NULL,
    score_b integer NOT NULL,
    estimated_remaining_seconds bigint
);
