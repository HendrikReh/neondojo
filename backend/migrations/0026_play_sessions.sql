CREATE TABLE play_sessions (
    id uuid PRIMARY KEY,
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    state jsonb NOT NULL,
    gnubg_path text NOT NULL,
    sgf text NOT NULL,
    native_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX play_sessions_recent ON play_sessions(updated_at DESC);
