CREATE TABLE anki_media (
    package text NOT NULL,
    name text NOT NULL,
    content_type text NOT NULL,
    content bytea NOT NULL,
    PRIMARY KEY (package, name)
);

CREATE TABLE anki_card_faces (
    card_id uuid PRIMARY KEY REFERENCES learning_cards(id) ON DELETE CASCADE,
    package text NOT NULL,
    css text NOT NULL,
    front text NOT NULL,
    back text NOT NULL,
    template_number integer NOT NULL CHECK (template_number > 0)
);

CREATE TABLE anki_source_decks (
    source_name text PRIMARY KEY,
    deck_id uuid NOT NULL UNIQUE REFERENCES decks(id) ON DELETE CASCADE
);
