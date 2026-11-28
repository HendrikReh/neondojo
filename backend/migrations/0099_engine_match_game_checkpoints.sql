CREATE TABLE engine_match_game_checkpoints (
    run_id uuid NOT NULL REFERENCES engine_match_runs(id) ON DELETE CASCADE,
    unit_index integer NOT NULL CHECK (unit_index BETWEEN 0 AND 1999),
    game_number integer NOT NULL CHECK (game_number BETWEEN 1 AND 1000),
    sgf_prefix_bytes integer NOT NULL CHECK (sgf_prefix_bytes >= 0),
    sgf_suffix text NOT NULL,
    state bytea NOT NULL,
    PRIMARY KEY (run_id, unit_index, game_number)
);
