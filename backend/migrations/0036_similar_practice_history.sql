CREATE TABLE similar_practice_summaries (
    id uuid PRIMARY KEY,
    completed_at timestamptz NOT NULL,
    total_positions integer NOT NULL CHECK (total_positions BETWEEN 1 AND 50),
    scored_moves integer NOT NULL CHECK (scored_moves BETWEEN 1 AND total_positions),
    missed_moves integer NOT NULL CHECK (missed_moves BETWEEN 0 AND scored_moves),
    total_equity_loss double precision NOT NULL CHECK (total_equity_loss >= 0 AND total_equity_loss < 'Infinity'::float8),
    is_retry boolean NOT NULL
);

CREATE INDEX similar_practice_summaries_recent ON similar_practice_summaries (completed_at DESC, id DESC);
