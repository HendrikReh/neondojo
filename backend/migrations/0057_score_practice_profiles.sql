CREATE TABLE score_practice_profiles (
    id uuid PRIMARY KEY,
    card_id uuid NOT NULL REFERENCES learning_cards(id) ON DELETE CASCADE,
    card_version bigint NOT NULL,
    plies integer NOT NULL,
    engine jsonb NOT NULL,
    expires_at timestamptz NOT NULL DEFAULT now() + interval '1 day'
);
CREATE INDEX score_practice_profiles_expiry ON score_practice_profiles(expires_at);
