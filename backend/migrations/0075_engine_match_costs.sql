-- Measured engine work per duel side and match: CPU time of all engine
-- threads and wall time of the decisions only, plus the side's checker turns.
-- Matches played before this measurement keep NULL for every cost column.
ALTER TABLE engine_match_results
    ADD COLUMN cpu_us_a bigint,
    ADD COLUMN wall_us_a bigint,
    ADD COLUMN moves_a integer,
    ADD COLUMN cpu_us_b bigint,
    ADD COLUMN wall_us_b bigint,
    ADD COLUMN moves_b integer,
    ADD CONSTRAINT engine_match_results_cost_a CHECK (
        (cpu_us_a IS NULL AND wall_us_a IS NULL AND moves_a IS NULL)
        OR (cpu_us_a >= 0 AND wall_us_a >= 0 AND moves_a >= 0)
    ),
    ADD CONSTRAINT engine_match_results_cost_b CHECK (
        (cpu_us_b IS NULL AND wall_us_b IS NULL AND moves_b IS NULL)
        OR (cpu_us_b >= 0 AND wall_us_b >= 0 AND moves_b >= 0)
    );
