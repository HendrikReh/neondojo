ALTER TABLE app_settings DROP CONSTRAINT app_settings_sage_analysis_depth_check;
ALTER TABLE app_settings ADD CONSTRAINT app_settings_sage_analysis_depth_check
    CHECK (analysis_engine <> 'bgsage' OR analysis_plies BETWEEN 1 AND 4);
