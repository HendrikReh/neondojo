-- Separate host-local thread overrides for playing. The existing columns keep
-- applying to analyses and engine duels. Until now one override applied to
-- both purposes, so existing values carry over to playing unchanged.
ALTER TABLE app_settings
    ADD COLUMN gnubg_play_threads integer
        CONSTRAINT app_settings_gnubg_play_threads_check CHECK (gnubg_play_threads >= 1),
    ADD COLUMN open_sage_play_threads integer
        CONSTRAINT app_settings_open_sage_play_threads_check CHECK (open_sage_play_threads >= 2);

UPDATE app_settings
SET gnubg_play_threads = gnubg_threads,
    open_sage_play_threads = open_sage_threads;
