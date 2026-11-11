-- Positions retain immutable board/player context, including after source removal.
-- NULL marks an invalid historical board so lazy backfill can make progress.
CREATE TABLE position_game_plans (
    position_uuid uuid NOT NULL REFERENCES positions(id) ON DELETE CASCADE,
    classifier text NOT NULL,
    game_plan text CHECK (game_plan IN ('pure_race','racing','attacking','priming','anchoring')),
    PRIMARY KEY (position_uuid,classifier)
);
CREATE INDEX position_game_plans_lookup ON position_game_plans(classifier,game_plan,position_uuid);
CREATE INDEX jobs_recurring_current ON jobs(match_uuid,source_revision,plies DESC,created_at DESC,id DESC)
    WHERE kind='match' AND status='completed';
