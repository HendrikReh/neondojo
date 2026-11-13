ALTER TABLE card_practice_rounds ADD COLUMN transfer jsonb;
ALTER TABLE card_practice_rounds ADD COLUMN completed_at timestamptz;
ALTER TABLE card_practice_items ADD COLUMN question jsonb;
ALTER TABLE card_practice_items ADD COLUMN diagnosis text;
ALTER TABLE card_practice_items ADD COLUMN lesson text;

CREATE TABLE transfer_practice_seen (
    signature text PRIMARY KEY
);
