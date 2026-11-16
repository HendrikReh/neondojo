-- Checked answers from recurring-mistake and decision-search practice, kept
-- so the practice history can show progress. No scheduling depends on them.
CREATE TABLE mistake_practice_attempts (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    attempted_at timestamptz NOT NULL DEFAULT now(),
    player text NOT NULL,
    game_plan text NOT NULL,
    mistake_kind text NOT NULL,
    verdict text NOT NULL CHECK (verdict IN ('best','playable','error','forced')),
    equity_loss double precision NOT NULL CHECK (equity_loss >= 0)
);
CREATE INDEX mistake_practice_attempts_time ON mistake_practice_attempts(attempted_at DESC);
