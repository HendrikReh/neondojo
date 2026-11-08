CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE TABLE matches (
    id uuid PRIMARY KEY,
    filename text NOT NULL,
    format text NOT NULL CHECK (format IN ('mat', 'sgf', 'xg')),
    sha256 text NOT NULL UNIQUE,
    white text NOT NULL,
    black text NOT NULL,
    match_length integer NOT NULL CHECK (match_length >= 0),
    position_count integer NOT NULL,
    original bytea NOT NULL,
    sgf text NOT NULL,
    search_text text GENERATED ALWAYS AS (lower(white || ' ' || black || ' ' || filename)) STORED,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX matches_search_trgm ON matches USING gin (search_text gin_trgm_ops);

CREATE TABLE positions (
    id uuid PRIMARY KEY,
    match_uuid uuid NOT NULL REFERENCES matches(id) ON DELETE CASCADE,
    ordinal integer NOT NULL,
    game_no integer NOT NULL,
    ply integer NOT NULL,
    player integer NOT NULL CHECK (player IN (0, 1)),
    dice integer[] NOT NULL CHECK (array_length(dice, 1) = 2),
    board integer[] NOT NULL CHECK (array_length(board, 1) = 26),
    position_id text NOT NULL,
    gnubg_match_id text NOT NULL,
    notation text NOT NULL,
    UNIQUE (match_uuid, ordinal)
);
CREATE INDEX positions_position_id ON positions(position_id, id);
CREATE INDEX positions_match_id ON positions(match_uuid, id);

CREATE TABLE jobs (
    id uuid PRIMARY KEY,
    position_uuid uuid NOT NULL REFERENCES positions(id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('analysis', 'dice')),
    plies integer NOT NULL CHECK (plies BETWEEN 0 AND 2),
    status text NOT NULL CHECK (status IN ('queued', 'running', 'completed', 'failed')),
    progress integer NOT NULL DEFAULT 0 CHECK (progress BETWEEN 0 AND 100),
    result jsonb,
    error text,
    attempts integer NOT NULL DEFAULT 0,
    lease_token uuid,
    lease_until timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX jobs_claim ON jobs(status, created_at);
CREATE UNIQUE INDEX jobs_one_active ON jobs(position_uuid, kind, plies) WHERE status IN ('queued', 'running');

CREATE TABLE job_events (
    id bigserial PRIMARY KEY,
    job_id uuid NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    event text NOT NULL,
    data jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX job_events_replay ON job_events(job_id, id);
