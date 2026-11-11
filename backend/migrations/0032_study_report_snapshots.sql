CREATE TABLE study_report_snapshots (
    id uuid PRIMARY KEY,
    session_id uuid NOT NULL REFERENCES play_sessions(id) ON DELETE CASCADE,
    game_no integer NOT NULL CHECK (game_no >= 0),
    library_match_id uuid REFERENCES matches(id) ON DELETE SET NULL,
    state jsonb NOT NULL CHECK (jsonb_typeof(state->'review')='object'),
    sgf text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE(session_id,game_no)
);
INSERT INTO study_report_snapshots(id,session_id,game_no,library_match_id,state,sgf,created_at,updated_at)
SELECT id,id,CASE WHEN (state->>'match_length')::int=0 THEN (state->>'game_no')::int ELSE 0 END,
       library_match_id,state,sgf,created_at,updated_at
FROM play_sessions WHERE state->>'phase'='match_over' AND jsonb_typeof(state->'review')='object';
CREATE INDEX study_report_snapshots_recent ON study_report_snapshots(updated_at DESC,id DESC);
