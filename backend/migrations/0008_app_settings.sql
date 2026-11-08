CREATE TABLE app_settings (
    singleton boolean PRIMARY KEY DEFAULT true CHECK (singleton),
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    player_name text NOT NULL DEFAULT '',
    analysis_engine text NOT NULL DEFAULT 'gnubg' CHECK (analysis_engine='gnubg'),
    analysis_plies integer NOT NULL DEFAULT 0 CHECK (analysis_plies BETWEEN 0 AND 2),
    play_engine text NOT NULL DEFAULT 'gnubg' CHECK (play_engine='gnubg'),
    play_plies integer NOT NULL DEFAULT 0 CHECK (play_plies BETWEEN 0 AND 2),
    gnubg_path text NOT NULL,
    gnubg_version text
);
-- Initialize lazily from the Engine injected into AppState, preserving GNUBG_BIN
-- and tests that deliberately inject an unavailable or fake executable.

ALTER TABLE jobs ADD COLUMN engine_config text NOT NULL DEFAULT 'legacy';
ALTER TABLE jobs ADD COLUMN engine_snapshot jsonb;
-- Existing jobs intentionally have no snapshot: workers keep using their injected
-- legacy engine, including when resuming a pre-migration checkpoint.
DROP INDEX jobs_one_active;
CREATE UNIQUE INDEX jobs_one_active ON jobs(position_uuid, kind, plies, engine_config)
    WHERE status IN ('queued','running');
DROP INDEX jobs_one_active_match;
CREATE UNIQUE INDEX jobs_one_active_match ON jobs(match_uuid, (coalesce(game_no,0)), plies, engine_config)
    WHERE kind='match' AND status IN ('queued','running');
