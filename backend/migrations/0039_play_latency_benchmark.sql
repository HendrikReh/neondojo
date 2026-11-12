ALTER TABLE benchmark_runs
    ALTER COLUMN reference DROP NOT NULL,
    ALTER COLUMN protocol DROP NOT NULL,
    ALTER COLUMN reference_sgf DROP NOT NULL,
    ADD COLUMN play_reference jsonb,
    ADD COLUMN completed_turns integer NOT NULL DEFAULT 0,
    ADD COLUMN average_turn_ms double precision,
    DROP CONSTRAINT benchmark_runs_check;

ALTER TABLE benchmark_runs
    ADD CONSTRAINT benchmark_workload CHECK (
        (play_reference IS NULL AND reference IS NOT NULL AND protocol IS NOT NULL
            AND reference_sgf IS NOT NULL AND completed_turns = 0 AND average_turn_ms IS NULL)
        OR (play_reference IS NOT NULL AND reference IS NULL AND protocol IS NULL
            AND reference_sgf IS NULL AND plies BETWEEN 0 AND 2 AND completed_games = 0
            AND analyzed_moves = 0 AND average_move_ms IS NULL
            AND (play_reference->>'total_turns')::integer IS NOT NULL
            AND completed_turns BETWEEN 0 AND (play_reference->>'total_turns')::integer)
    ),
    ADD CONSTRAINT benchmark_result CHECK (
        (status = 'completed' AND finished_at IS NOT NULL AND total_elapsed_ms IS NOT NULL
            AND total_elapsed_ms > 0 AND total_elapsed_ms < 'Infinity'::float8
            AND ((play_reference IS NULL AND completed_games = 10 AND analyzed_moves = 460
                AND average_move_ms IS NOT NULL AND average_move_ms > 0 AND average_move_ms < 'Infinity'::float8)
                OR (play_reference IS NOT NULL AND completed_turns = (play_reference->>'total_turns')::integer
                    AND average_turn_ms IS NOT NULL AND average_turn_ms > 0 AND average_turn_ms < 'Infinity'::float8)))
        OR (status <> 'completed' AND total_elapsed_ms IS NULL AND average_move_ms IS NULL AND average_turn_ms IS NULL)
    );
