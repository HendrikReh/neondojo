-- The classifier gained an Opening label on top of the pinned rules. Cached
-- labels are keyed by classifier and are recomputed on demand.
ALTER TABLE position_game_plans DROP CONSTRAINT position_game_plans_game_plan_check;
ALTER TABLE position_game_plans ADD CONSTRAINT position_game_plans_game_plan_check
    CHECK (game_plan IN ('opening','pure_race','racing','attacking','priming','anchoring'));
