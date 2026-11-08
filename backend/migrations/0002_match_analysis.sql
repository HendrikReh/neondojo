ALTER TABLE jobs ALTER COLUMN position_uuid DROP NOT NULL;
ALTER TABLE jobs ADD COLUMN match_uuid uuid REFERENCES matches(id) ON DELETE CASCADE;
ALTER TABLE jobs DROP CONSTRAINT jobs_kind_check;
ALTER TABLE jobs ADD CONSTRAINT jobs_kind_check CHECK (kind IN ('analysis', 'dice', 'match'));
ALTER TABLE jobs ADD CONSTRAINT jobs_target_check CHECK (
    (kind = 'match' AND match_uuid IS NOT NULL AND position_uuid IS NULL)
    OR (kind IN ('analysis', 'dice') AND position_uuid IS NOT NULL AND match_uuid IS NULL)
);
ALTER TABLE jobs ADD COLUMN total_games integer NOT NULL DEFAULT 0 CHECK (total_games >= 0);
ALTER TABLE jobs ADD COLUMN completed_games integer NOT NULL DEFAULT 0 CHECK (completed_games BETWEEN 0 AND total_games);
CREATE UNIQUE INDEX jobs_one_active_match ON jobs(match_uuid, plies) WHERE kind='match' AND status IN ('queued','running');
CREATE INDEX jobs_match_history ON jobs(match_uuid, created_at DESC, id DESC) WHERE kind='match';

-- A checkpoint and its decisions are committed together under the worker's lease.
CREATE TABLE match_analysis_games (
    job_id uuid NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    game_no integer NOT NULL CHECK (game_no > 0),
    PRIMARY KEY (job_id, game_no)
);
CREATE TABLE move_analyses (
    id bigserial PRIMARY KEY,
    job_id uuid NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    position_uuid uuid REFERENCES positions(id) ON DELETE CASCADE,
    game_no integer NOT NULL CHECK (game_no > 0),
    decision_no integer NOT NULL CHECK (decision_no > 0),
    player integer NOT NULL CHECK (player IN (0,1)),
    kind text NOT NULL CHECK (kind IN ('checker','cube')),
    equity_loss double precision NOT NULL CHECK (equity_loss >= 0 AND equity_loss < 'Infinity'::float8),
    data jsonb NOT NULL,
    UNIQUE (job_id, game_no, decision_no)
);
CREATE INDEX move_analyses_page ON move_analyses(job_id, id);
CREATE INDEX move_analyses_player ON move_analyses(job_id, player, id);
CREATE INDEX move_analyses_errors ON move_analyses(job_id, id) WHERE equity_loss >= 0.04;
