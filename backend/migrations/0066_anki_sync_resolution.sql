ALTER TABLE anki_sync_history
    ADD COLUMN resolved_at timestamptz,
    ADD CHECK (resolved_at IS NULL OR (outcome = 'failed' AND resolved_at >= finished_at));
