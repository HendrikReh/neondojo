CREATE TABLE anki_auto_sync (
    deck_id uuid PRIMARY KEY REFERENCES decks(id) ON DELETE CASCADE,
    link_id uuid NOT NULL,
    profile text NOT NULL,
    collection_key text NOT NULL,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    last_synced_at timestamptz,
    last_received_at timestamptz,
    last_outcome text CHECK (last_outcome IN ('synced', 'no_matches', 'failed'))
);

CREATE INDEX anki_auto_sync_due ON anki_auto_sync(collection_key, next_attempt_at, deck_id);
