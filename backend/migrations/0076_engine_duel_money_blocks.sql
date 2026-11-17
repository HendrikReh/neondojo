-- Cubeless money blocks: score_a/score_b hold the points each side won in the
-- block, which may tie. outcomes counts the block's games won by A as single,
-- gammon and backgammon, then by B. Match results keep outcomes NULL and must
-- still have a winner.
ALTER TABLE engine_match_results
    ADD COLUMN outcomes integer[],
    DROP CONSTRAINT engine_match_results_check,
    ADD CONSTRAINT engine_match_results_check CHECK (
        score_b >= 0 AND (outcomes IS NOT NULL OR score_a <> score_b)
    ),
    ADD CONSTRAINT engine_match_results_outcomes CHECK (
        outcomes IS NULL OR (
            cardinality(outcomes) = 6
            AND array_ndims(outcomes) = 1
            AND 0 <= ALL (outcomes)
            AND score_a = outcomes[1] + 2 * outcomes[2] + 3 * outcomes[3]
            AND score_b = outcomes[4] + 2 * outcomes[5] + 3 * outcomes[6]
            AND games = outcomes[1] + outcomes[2] + outcomes[3]
                + outcomes[4] + outcomes[5] + outcomes[6]
        )
    );
