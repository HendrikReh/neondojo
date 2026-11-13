ALTER TABLE learning_cards ADD COLUMN anki_review_count integer NOT NULL DEFAULT 0
    CHECK (anki_review_count >= 0);

CREATE TABLE anki_received_reviews (
    card_id uuid NOT NULL REFERENCES learning_cards(id) ON DELETE CASCADE,
    review_id bigint NOT NULL CHECK (review_id > 0),
    rating text NOT NULL CHECK (rating IN ('again','hard','good','easy')),
    PRIMARY KEY (card_id, review_id)
);

CREATE TABLE anki_review_exports (
    card_id uuid NOT NULL,
    request_id uuid NOT NULL,
    review_id bigint NOT NULL UNIQUE CHECK (review_id > 0),
    PRIMARY KEY (card_id, request_id),
    FOREIGN KEY (card_id, request_id) REFERENCES card_reviews(card_id, request_id) ON DELETE CASCADE
);
