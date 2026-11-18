-- Host-local limit for simultaneously running analysis jobs. NULL keeps the
-- previous behavior of one job at a time. The upper bound lives in
-- shared/configuration-options.json and is validated by the backend.
ALTER TABLE app_settings
    ADD COLUMN parallel_analysis_jobs integer
        CONSTRAINT app_settings_parallel_analysis_jobs_check CHECK (parallel_analysis_jobs >= 1);
