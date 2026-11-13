ALTER TABLE learning_cards
    ADD COLUMN tags text[] NOT NULL DEFAULT '{}',
    ADD COLUMN tags_version bigint NOT NULL DEFAULT 0 CHECK (tags_version >= 0);
CREATE INDEX learning_cards_tags ON learning_cards USING gin(tags);

CREATE TABLE card_topics (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    filter jsonb NOT NULL
);
CREATE UNIQUE INDEX card_topics_name ON card_topics(lower(name));
