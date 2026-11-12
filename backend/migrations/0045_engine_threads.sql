ALTER TABLE app_settings
    ADD COLUMN gnubg_threads integer CHECK (gnubg_threads BETWEEN 1 AND 48),
    ADD COLUMN open_sage_threads integer CHECK (open_sage_threads BETWEEN 2 AND 48);
