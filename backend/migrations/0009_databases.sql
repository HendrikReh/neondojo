-- Logical databases own their matches, source bytes, notes and analysis copies.
CREATE TABLE databases (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (char_length(btrim(name)) BETWEEN 1 AND 120),
    description text NOT NULL DEFAULT '' CHECK (char_length(description) <= 2000),
    version bigint NOT NULL DEFAULT 1 CHECK (version > 0),
    is_default boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX databases_single_default ON databases(is_default) WHERE is_default;
INSERT INTO databases(id,name,is_default) VALUES ('00000000-0000-0000-0000-000000000001','My matches',true);
ALTER TABLE matches ADD COLUMN database_id uuid NOT NULL DEFAULT '00000000-0000-0000-0000-000000000001' REFERENCES databases(id);
ALTER TABLE matches DROP CONSTRAINT matches_sha256_key;
ALTER TABLE matches ADD CONSTRAINT matches_database_sha256_key UNIQUE(database_id,sha256);
ALTER TABLE matches ADD COLUMN display_name text NOT NULL DEFAULT '' CHECK(char_length(display_name)<=240);
ALTER TABLE matches ADD COLUMN played_on date;
ALTER TABLE matches ADD COLUMN tournament text CHECK(char_length(tournament)<=240);
ALTER TABLE matches ADD COLUMN round text CHECK(char_length(round)<=120);
ALTER TABLE matches ADD COLUMN place text CHECK(char_length(place)<=240);
ALTER TABLE matches ADD COLUMN result_state text NOT NULL DEFAULT 'unknown' CHECK(result_state IN ('unknown','ongoing','completed'));
ALTER TABLE matches ADD COLUMN white_score integer CHECK(white_score>=0);
ALTER TABLE matches ADD COLUMN black_score integer CHECK(black_score>=0);
ALTER TABLE matches ADD COLUMN metadata_version bigint NOT NULL DEFAULT 0 CHECK(metadata_version>=0);
CREATE INDEX matches_database_created ON matches(database_id,created_at,id);
CREATE INDEX matches_database_played ON matches(database_id,played_on,id);
CREATE INDEX matches_database_name ON matches(database_id,display_name,id);
CREATE INDEX matches_database_players ON matches(database_id,white,black,id);
CREATE INDEX matches_database_length ON matches(database_id,match_length,id);
CREATE INDEX matches_tournament_trgm ON matches USING gin(lower(tournament) gin_trgm_ops);
CREATE INDEX matches_display_name_trgm ON matches USING gin(lower(display_name) gin_trgm_ops);
CREATE INDEX matches_white_trgm ON matches USING gin(lower(white) gin_trgm_ops);
CREATE INDEX matches_black_trgm ON matches USING gin(lower(black) gin_trgm_ops);
-- Backfilled once from canonical SGF. A manual save marks the source as handled.
ALTER TABLE matches ADD COLUMN source_metadata_loaded boolean NOT NULL DEFAULT false;
