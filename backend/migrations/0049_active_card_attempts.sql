ALTER TABLE review_sessions
    ADD COLUMN attempt_request_id uuid,
    ADD COLUMN attempt_input jsonb,
    ADD COLUMN attempt_result jsonb,
    ADD CONSTRAINT review_sessions_complete_attempt CHECK (
        num_nonnulls(attempt_request_id, attempt_input, attempt_result) IN (0, 3)
    );

CREATE UNIQUE INDEX review_sessions_first_attempt
    ON review_sessions (card_id, card_version)
    WHERE attempt_request_id IS NOT NULL;
