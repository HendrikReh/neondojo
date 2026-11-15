ALTER TABLE anki_auto_sync
    ADD COLUMN paused boolean NOT NULL DEFAULT false,
    ADD COLUMN running_until timestamptz;
