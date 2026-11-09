ALTER TABLE jobs DROP CONSTRAINT jobs_plies_check;
ALTER TABLE jobs ADD CONSTRAINT jobs_plies_check CHECK (plies BETWEEN 0 AND 4);

ALTER TABLE app_settings DROP CONSTRAINT app_settings_analysis_plies_check;
ALTER TABLE app_settings ADD CONSTRAINT app_settings_analysis_plies_check
    CHECK (analysis_plies BETWEEN 0 AND 4);

-- Keep the 2-ply default, saved preferences and their revisions unchanged.
-- Existing jobs, engine snapshots and results retain their original depth.
-- The separate future play profile remains 0..2.
