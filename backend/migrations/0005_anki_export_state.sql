-- Anki's default importer only updates existing notes when their modification time increases.
-- A rendered-content fingerprint covers annotations, translations, and media as well as analysis.
-- Keeping this outside learning_cards leaves local review versions and schedules untouched.
CREATE TABLE learning_card_exports (
    card_id uuid PRIMARY KEY REFERENCES learning_cards(id) ON DELETE CASCADE,
    fingerprint text NOT NULL CHECK (length(fingerprint) = 64),
    modified bigint NOT NULL CHECK (modified > 0)
);
