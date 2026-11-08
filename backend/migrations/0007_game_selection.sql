-- Catalog source games independently of their checker positions: empty and cube-only
-- games are navigable and can be analyzed in their original match context.
CREATE TABLE match_games (
    match_uuid uuid NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    game_no integer NOT NULL CHECK (game_no > 0),
    position_count integer NOT NULL CHECK (position_count >= 0),
    white_score integer NOT NULL CHECK (white_score >= 0),
    black_score integer NOT NULL CHECK (black_score >= 0),
    crawford boolean NOT NULL,
    PRIMARY KEY (match_uuid, game_no)
);
CREATE INDEX positions_match_game_ordinal ON positions(match_uuid, game_no, ordinal);

ALTER TABLE jobs ADD COLUMN game_no integer;
ALTER TABLE jobs ADD CONSTRAINT jobs_game_scope_check
    CHECK (game_no IS NULL OR (kind='match' AND game_no > 0 AND total_games=1));
DROP INDEX jobs_one_active_match;
CREATE UNIQUE INDEX jobs_one_active_match ON jobs(match_uuid, (coalesce(game_no,0)), plies)
    WHERE kind='match' AND status IN ('queued','running');
DROP INDEX jobs_match_history;
CREATE INDEX jobs_match_history ON jobs(match_uuid, (coalesce(game_no,0)), created_at DESC, id DESC)
    WHERE kind='match';
CREATE INDEX move_analyses_game_page ON move_analyses(job_id, game_no, id);
