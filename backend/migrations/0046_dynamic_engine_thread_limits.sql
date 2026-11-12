ALTER TABLE app_settings
    DROP CONSTRAINT app_settings_gnubg_threads_check,
    DROP CONSTRAINT app_settings_open_sage_threads_check,
    ADD CONSTRAINT app_settings_gnubg_threads_check CHECK (gnubg_threads >= 1),
    ADD CONSTRAINT app_settings_open_sage_threads_check CHECK (open_sage_threads >= 2);
