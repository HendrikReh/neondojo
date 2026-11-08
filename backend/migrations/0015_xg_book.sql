-- The official XG data is a separate position-reference source, not BGDB data.
ALTER TABLE opening_book_settings
    DROP CONSTRAINT opening_book_settings_provider_check;
ALTER TABLE opening_book_settings
    ADD CONSTRAINT opening_book_settings_provider_check
        CHECK (provider IN ('off','backgammondb','automatic','xg'));
ALTER TABLE opening_book_settings
    ADD COLUMN xg_path text NOT NULL DEFAULT '';
