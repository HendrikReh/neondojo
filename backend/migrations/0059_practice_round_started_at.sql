ALTER TABLE card_practice_rounds ADD COLUMN started_at timestamptz NOT NULL DEFAULT now();
