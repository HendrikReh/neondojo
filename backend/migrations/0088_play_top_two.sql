ALTER TABLE app_settings
    ADD COLUMN play_top_two boolean NOT NULL DEFAULT false,
    ADD CONSTRAINT app_settings_play_top_two_profile
        CHECK (NOT play_top_two OR (play_engine = 'bgsage' AND play_plies = 2));
