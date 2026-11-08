CREATE TABLE decks (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    created_at timestamptz NOT NULL DEFAULT now()
);
-- A decision belongs to the library once; membership and review history are independent.
CREATE TABLE learning_cards (
    id uuid PRIMARY KEY,
    signature text NOT NULL UNIQUE,
    payload jsonb NOT NULL,
    stability double precision CHECK (stability > 0 AND stability < 'Infinity'::float8),
    difficulty double precision CHECK (difficulty >= 1 AND difficulty <= 10),
    last_review timestamptz,
    due_at timestamptz NOT NULL DEFAULT now(),
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    review_count integer NOT NULL DEFAULT 0 CHECK (review_count >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK ((stability IS NULL AND difficulty IS NULL AND last_review IS NULL) OR
           (stability IS NOT NULL AND difficulty IS NOT NULL AND last_review IS NOT NULL))
);
CREATE INDEX learning_cards_due ON learning_cards(due_at,id);
CREATE TABLE deck_cards (
    deck_id uuid NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
    card_id uuid NOT NULL REFERENCES learning_cards(id) ON DELETE CASCADE,
    PRIMARY KEY(deck_id,card_id)
);
CREATE INDEX deck_cards_card ON deck_cards(card_id);
CREATE TABLE review_sessions (
    id uuid PRIMARY KEY,
    card_id uuid NOT NULL REFERENCES learning_cards(id) ON DELETE CASCADE,
    card_version bigint NOT NULL,
    expires_at timestamptz NOT NULL
);
CREATE INDEX review_sessions_card ON review_sessions(card_id);
CREATE TABLE card_reviews (
    card_id uuid NOT NULL REFERENCES learning_cards(id) ON DELETE CASCADE,
    request_id uuid NOT NULL,
    session_id uuid NOT NULL UNIQUE REFERENCES review_sessions(id),
    rating text NOT NULL CHECK (rating IN ('again','hard','good','easy')),
    result jsonb NOT NULL,
    reviewed_at timestamptz NOT NULL,
    PRIMARY KEY(card_id,request_id)
);
CREATE INDEX card_reviews_history ON card_reviews(card_id,reviewed_at);
