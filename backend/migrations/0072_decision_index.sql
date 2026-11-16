-- Derived index of the decisions in each match's displayed analysis. It is
-- rebuilt per match whenever that match's fingerprint changes, so it holds
-- no information that the source analyses do not.
CREATE TABLE decision_index_matches (
    match_id uuid PRIMARY KEY REFERENCES matches(id) ON DELETE CASCADE,
    -- Displayed analysis, source revision, player names and index version.
    fingerprint text NOT NULL,
    indexed_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE decision_index (
    match_id uuid NOT NULL REFERENCES decision_index_matches(match_id) ON DELETE CASCADE,
    seq integer NOT NULL,
    game_no integer NOT NULL,
    decision_no integer NOT NULL,
    -- Lowercase, trimmed name of the player who decided.
    player_name text NOT NULL,
    question_kind text NOT NULL CHECK (question_kind IN ('checker','cube_offer','cube_response')),
    game_plan text NOT NULL,
    -- Decision category that counts as an opportunity; null without a defined denominator.
    opportunity text,
    -- Error type of this decision if its loss is positive.
    mistake_kind text NOT NULL,
    equity_loss double precision NOT NULL,
    match_length integer NOT NULL,
    away_own integer,
    away_opponent integer,
    cube_value integer NOT NULL,
    cube_owner text NOT NULL CHECK (cube_owner IN ('self','opponent','centered')),
    played_on date NOT NULL,
    -- The search result as returned by the API.
    hit jsonb NOT NULL,
    PRIMARY KEY (match_id, seq)
);
CREATE INDEX decision_index_player_loss
    ON decision_index (player_name, equity_loss DESC, match_id, game_no, decision_no);
CREATE INDEX decision_index_player_group
    ON decision_index (player_name, game_plan, mistake_kind);
