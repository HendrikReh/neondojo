CREATE TABLE card_practice_rounds (
    id uuid PRIMARY KEY,
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    pass smallint NOT NULL DEFAULT 1 CHECK (pass IN (1, 2))
);

CREATE TABLE card_practice_items (
    round_id uuid NOT NULL REFERENCES card_practice_rounds(id) ON DELETE CASCADE,
    pass smallint NOT NULL CHECK (pass IN (1, 2)),
    ordinal integer NOT NULL CHECK (ordinal >= 0),
    card_id uuid NOT NULL,
    answered_by uuid,
    submitted jsonb,
    result jsonb,
    marked boolean NOT NULL DEFAULT false,
    advanced boolean NOT NULL DEFAULT false,
    PRIMARY KEY (round_id, pass, ordinal),
    UNIQUE (round_id, pass, card_id),
    CHECK ((answered_by IS NULL) = (result IS NULL)),
    CHECK (NOT marked OR result IS NOT NULL)
);
