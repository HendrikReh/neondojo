ALTER TABLE anki_auto_sync
    ADD COLUMN received_reviews bigint NOT NULL DEFAULT 0 CHECK (received_reviews >= 0),
    ADD COLUMN sent_reviews bigint NOT NULL DEFAULT 0 CHECK (sent_reviews >= 0),
    ADD COLUMN last_problem text CHECK (
        last_problem IN ('unreachable', 'invalid_deck', 'conflict', 'timeout', 'anki_error', 'internal_error')
    );
