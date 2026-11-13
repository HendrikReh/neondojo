CREATE TABLE anki_sync_history (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    deck_id uuid NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
    profile text NOT NULL,
    finished_at timestamptz NOT NULL DEFAULT now(),
    received_reviews bigint NOT NULL CHECK (received_reviews >= 0),
    sent_reviews bigint NOT NULL CHECK (sent_reviews >= 0),
    outcome text NOT NULL CHECK (outcome IN ('synced', 'no_matches', 'failed')),
    problem text CHECK (
        problem IN ('unreachable', 'invalid_deck', 'conflict', 'timeout', 'anki_error', 'internal_error')
    ),
    error_detail text CHECK (char_length(error_detail) <= 300),
    CHECK ((outcome = 'failed') = (problem IS NOT NULL AND error_detail IS NOT NULL))
);

CREATE INDEX anki_sync_history_recent ON anki_sync_history(deck_id, finished_at DESC, id DESC);
