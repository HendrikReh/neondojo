ALTER TABLE jobs DROP CONSTRAINT jobs_plies_check;
ALTER TABLE jobs ADD CONSTRAINT jobs_plies_check CHECK (plies BETWEEN 0 AND 3);

ALTER TABLE app_settings DROP CONSTRAINT app_settings_analysis_plies_check;
ALTER TABLE app_settings ADD CONSTRAINT app_settings_analysis_plies_check
    CHECK (analysis_plies BETWEEN 0 AND 3);
ALTER TABLE app_settings ALTER COLUMN analysis_plies SET DEFAULT 2;

-- Upgrade only the original, never-saved analysis default. A saved revision
-- owns its selected depth, including an explicit 0-ply preference. Increment
-- the revision so an open pre-upgrade settings form cannot overwrite the change.
UPDATE app_settings SET analysis_plies = 2, version = version + 1
WHERE analysis_plies = 0 AND version = 0;

-- Existing jobs retain their requested depth, snapshots and results. The play
-- profile is independent and keeps its existing 0..2 range and default.
