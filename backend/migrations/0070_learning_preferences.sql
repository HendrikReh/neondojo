-- One row of study scheduling preferences. New flashcards are due as soon
-- as they are created, so a daily allowance keeps the first session small.
CREATE TABLE learning_preferences (
    id boolean PRIMARY KEY DEFAULT true CHECK (id),
    new_cards_per_day integer NOT NULL DEFAULT 10
        CHECK (new_cards_per_day BETWEEN 0 AND 1000),
    updated_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO learning_preferences DEFAULT VALUES;
