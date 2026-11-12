-- Keep existing defaults and user settings. The application additionally checks
-- the installed engine's verified capability before accepting a 1-ply profile.
ALTER TABLE app_settings DROP CONSTRAINT app_settings_check;
ALTER TABLE app_settings DROP CONSTRAINT app_settings_check1;
ALTER TABLE app_settings ADD CONSTRAINT app_settings_sage_analysis_depth_check
    CHECK (analysis_engine <> 'bgsage' OR analysis_plies IN (1, 2));
ALTER TABLE app_settings ADD CONSTRAINT app_settings_sage_play_depth_check
    CHECK (play_engine <> 'bgsage' OR play_plies IN (1, 2));
