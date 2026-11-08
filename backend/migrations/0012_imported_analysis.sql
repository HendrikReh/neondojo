-- Source evaluations are immutable provenance, independently of GNUbg jobs.
ALTER TABLE matches ADD COLUMN imported_analysis_loaded boolean NOT NULL DEFAULT false;
CREATE TABLE imported_analyses (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    match_uuid uuid NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    position_uuid uuid REFERENCES positions(id) ON DELETE SET NULL,
    source_ordinal integer NOT NULL CHECK(source_ordinal >= 0),
    game_no integer NOT NULL,
    source_move_no integer NOT NULL CHECK(source_move_no >= 0),
    kind text NOT NULL,
    data jsonb NOT NULL,
    UNIQUE(match_uuid,source_ordinal),
    FOREIGN KEY(match_uuid,game_no) REFERENCES match_games(match_uuid,game_no) ON DELETE CASCADE
);
CREATE INDEX imported_analyses_position ON imported_analyses(position_uuid) WHERE position_uuid IS NOT NULL;
CREATE INDEX imported_analyses_move ON imported_analyses(match_uuid,game_no,source_move_no);
