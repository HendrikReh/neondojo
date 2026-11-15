-- Selection metadata references immutable result identities; result history stays
-- in jobs/checkpoints/imported_analyses and is never replaced by this catalog.
CREATE TABLE analysis_selections (
    match_id uuid PRIMARY KEY REFERENCES matches(id) ON DELETE CASCADE,
    source_revision bigint NOT NULL DEFAULT 0 CHECK (source_revision>=0),
    analysis_id text NOT NULL CHECK (analysis_id ~ '^(job|imported):[0-9a-f-]{36}$'),
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- Individual position jobs must snapshot their owning match as match jobs do.
CREATE OR REPLACE FUNCTION snapshot_job_source_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.source_revision = -1 THEN
        NEW.source_revision := 0;
        IF NEW.match_uuid IS NOT NULL THEN
            SELECT source_revision INTO NEW.source_revision FROM matches WHERE id=NEW.match_uuid;
        ELSIF NEW.position_uuid IS NOT NULL THEN
            SELECT m.source_revision INTO NEW.source_revision
            FROM positions p JOIN matches m ON m.id=p.match_uuid WHERE p.id=NEW.position_uuid;
        END IF;
    END IF;
    RETURN NEW;
END $$;

CREATE INDEX jobs_catalog_match ON jobs(match_uuid,created_at DESC,id DESC)
    WHERE kind='match';
CREATE INDEX jobs_catalog_position ON jobs(position_uuid,created_at DESC,id DESC)
    WHERE kind='analysis' AND status='completed' AND result IS NOT NULL;

-- A source record can contain original summary/probability values without a
-- candidate list or comparable units. The legacy HedgeHog all-default block is
-- a documented absence marker, while genuine recorded zero values stay visible.
CREATE FUNCTION imported_analysis_has_values(data jsonb) RETURNS boolean
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
SELECT coalesce(
    jsonb_path_exists(data,'$.candidates[*].equity ? (@.type() == "number")')
    OR jsonb_path_exists(data,'$.candidates[*].probabilities.* ? (@.type() == "number")')
    OR jsonb_path_exists(data,'$.raw.best_equity ? (@.type() == "number")')
    OR jsonb_path_exists(data,'$.raw.equity_loss ? (@.type() == "number" && @ >= 0)')
    OR jsonb_path_exists(data,'$.cube.no_double ? (@.type() == "number")')
    OR jsonb_path_exists(data,'$.cube.double_take ? (@.type() == "number")')
    OR jsonb_path_exists(data,'$.cube.double_pass ? (@.type() == "number")')
    OR jsonb_path_exists(data,'$.cube.equity_loss ? (@.type() == "number" && @ >= 0)')
    OR jsonb_path_exists(data,'$.cube.cubeful_take_value ? (@.type() == "number")')
    OR jsonb_path_exists(data,'$.cube.probabilities.* ? (@.type() == "number")')
    OR jsonb_path_exists(data,'$.cube.source.** ? (@.type() == "number")'), false)
AND NOT coalesce(data->>'engine'='Unknown' AND data->>'method'='unknown'
    AND data->'depth'='null'::jsonb
    AND data#>>'{raw,analysis_id}'='b86cd1b0-4d12-80e6-86ba-511a59c022f0'
    AND data#>'{raw,currency}'='null'::jsonb
    AND data#>'{raw,best_equity}'='0'::jsonb
    AND data#>'{raw,equity_loss}'='0'::jsonb,false)
$$;

-- One authoritative source record per decision drives all selected-result
-- consumers. Keep every original row and prefer an actual value over an empty
-- legacy placeholder; otherwise preserve source order, never engine or depth.
CREATE VIEW primary_imported_analyses AS
SELECT DISTINCT ON (match_uuid,position_uuid,game_no,source_move_no,kind) *
FROM imported_analyses
ORDER BY match_uuid,position_uuid,game_no,source_move_no,kind,
         imported_analysis_has_values(data) DESC,source_ordinal ASC,id ASC;

-- One eligibility rule, shared by the API resolver and set-based library reads.
-- Failed/running native work is selectable only when durable checkpoints exist.
-- Position candidates and imported evaluations preserve their original units.
CREATE VIEW analysis_catalog_sources AS
SELECT m.id AS match_id,'job:'||j.id::text AS analysis_id,'native'::text AS source,
       CASE WHEN j.game_no IS NULL THEN 'match' ELSE 'game' END AS scope,
       j.id AS job_id,j.created_at,j.status,
       (j.status='completed' AND j.completed_games=j.total_games
        AND checkpoints.games>=j.total_games AND NOT m.collection_only) AS complete,
       counts.checker_count,counts.cube_count,
       coalesce(nullif(j.result->>'engine',''),
           CASE WHEN j.engine_snapshot->>'engine_id'='bgsage' THEN 'Open Sage' ELSE 'GNU Backgammon' END) AS engine,
       j.plies::text||'-ply cubeful' AS depth,j.game_no,NULL::uuid AS position_id
FROM matches m JOIN jobs j ON j.match_uuid=m.id AND j.source_revision=m.source_revision AND j.kind='match'
JOIN LATERAL (SELECT count(*) AS games FROM match_analysis_games c WHERE c.job_id=j.id
    AND (j.game_no IS NULL OR c.game_no=j.game_no)) checkpoints ON checkpoints.games>0
CROSS JOIN LATERAL (
    SELECT count(*) FILTER(WHERE a.kind='checker') AS checker_count,
           count(*) FILTER(WHERE a.kind='cube') AS cube_count
    FROM move_analyses a JOIN match_analysis_games c ON c.job_id=a.job_id AND c.game_no=a.game_no
    WHERE a.job_id=j.id AND (j.game_no IS NULL OR a.game_no=j.game_no)
) counts
WHERE j.status IN ('completed','running','failed')
AND (counts.checker_count+counts.cube_count>0 OR EXISTS (
    SELECT 1 FROM match_analysis_games c WHERE c.job_id=j.id
    AND (j.game_no IS NULL OR c.game_no=j.game_no)
    AND c.summary->>'completed_games'='1' AND c.summary->>'total_games'='1'))
AND NOT EXISTS (
    SELECT 1 FROM play_sessions live WHERE live.library_match_id=m.id
    AND coalesce(live.state->>'play_mode','competition')='competition'
    AND live.state->>'phase'<>'match_over'
    AND NOT (live.state->>'match_length'='0' AND live.state->>'phase'='game_over'))
UNION ALL
SELECT m.id,'job:'||j.id::text,'native','position',j.id,j.created_at,j.status,true,
       1::bigint,0::bigint,coalesce(nullif(j.result->>'engine',''),
           CASE WHEN j.engine_snapshot->>'engine_id'='bgsage' THEN 'Open Sage' ELSE 'GNU Backgammon' END),
       j.plies::text||'-ply',p.game_no,p.id
FROM matches m JOIN positions p ON p.match_uuid=m.id
JOIN jobs j ON j.position_uuid=p.id AND j.kind='analysis' AND j.source_revision=m.source_revision
WHERE j.status='completed' AND jsonb_typeof(j.result->'candidates')='array'
    AND jsonb_array_length(CASE WHEN jsonb_typeof(j.result->'candidates')='array' THEN j.result->'candidates' ELSE '[]'::jsonb END)>0 AND NOT EXISTS (
    SELECT 1 FROM play_sessions live WHERE live.library_match_id=m.id
    AND coalesce(live.state->>'play_mode','competition')='competition'
    AND live.state->>'phase'<>'match_over'
    AND NOT (live.state->>'match_length'='0' AND live.state->>'phase'='game_over'))
UNION ALL
SELECT m.id,'imported:'||m.id::text,'imported','match',NULL::uuid,m.created_at,'completed',false,
       imported.checker_count,imported.cube_count,imported.engine,imported.depth,NULL::integer,NULL::uuid
FROM matches m JOIN LATERAL (
    SELECT count(*) FILTER(WHERE a.kind='checker') AS checker_count,
           count(*) FILTER(WHERE a.kind='cube') AS cube_count,
           coalesce(string_agg(DISTINCT nullif(concat_ws(' ',a.data->>'engine',a.data->>'engine_version'),''),', '
               ORDER BY nullif(concat_ws(' ',a.data->>'engine',a.data->>'engine_version'),'')),'Imported') AS engine,
           string_agg(DISTINCT a.data->>'depth',', ' ORDER BY a.data->>'depth') AS depth
    FROM primary_imported_analyses a WHERE a.match_uuid=m.id AND a.kind IN ('checker','cube')
        AND imported_analysis_has_values(a.data)
) imported ON imported.checker_count+imported.cube_count>0
WHERE NOT EXISTS (
    SELECT 1 FROM play_sessions live WHERE live.library_match_id=m.id
    AND coalesce(live.state->>'play_mode','competition')='competition'
    AND live.state->>'phase'<>'match_over'
    AND NOT (live.state->>'match_length'='0' AND live.state->>'phase'='game_over'));

-- A stale explicit selection deliberately produces no effective row. Never
-- substitute another engine or import behind the user's persisted choice.
CREATE VIEW effective_analysis_selections AS
SELECT m.id AS match_id,chosen.analysis_id,chosen.job_id,chosen.source
FROM matches m
LEFT JOIN analysis_selections selected ON selected.match_id=m.id
JOIN LATERAL (
    SELECT a.analysis_id,a.job_id,a.source
    FROM analysis_catalog_sources a WHERE a.match_id=m.id
    AND (CASE WHEN selected.analysis_id IS NOT NULL THEN a.analysis_id=selected.analysis_id AND selected.source_revision=m.source_revision
         ELSE a.status='completed' AND (a.source='imported' OR a.scope<>'match' OR a.complete OR m.collection_only) END)
    ORDER BY CASE WHEN a.source='native' AND a.scope='match' AND a.complete THEN 0
                  WHEN a.source='imported' THEN 1 ELSE 2 END,
             a.created_at DESC,a.analysis_id DESC
    LIMIT 1
) chosen ON true;

-- Derived, versioned validation cache. Original imported records stay untouched.
CREATE TABLE imported_analysis_projections (
    imported_analysis_id uuid PRIMARY KEY REFERENCES imported_analyses(id) ON DELETE CASCADE,
    revision integer NOT NULL,
    source_revision bigint NOT NULL DEFAULT 0 CHECK (source_revision>=0),
    projection jsonb
);

-- Calculation preferences no longer filter any saved-result relation.
DROP VIEW selected_engine_jobs;
