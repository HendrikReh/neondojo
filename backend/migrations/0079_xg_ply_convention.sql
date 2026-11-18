-- One ply convention for every engine: the XG / Open Sage convention, where
-- 1-ply is the raw neural-net evaluation. GNUbg counts that evaluation as
-- 0-ply, so every stored GNUbg depth moves up by one level. The backend
-- converts to GNUbg's native level only where it writes commands for GNUbg.
--
-- Unchanged on purpose: source text and identifiers. Archived duel SGFs and
-- the match names and events read from them, the raw tokens of imported
-- analyses, benchmark protocol ids (they name GNUbg's native filter level),
-- and historical snapshots of finished reviews and practice attempts.

CREATE FUNCTION pg_temp.shift_label(label text) RETURNS text
LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE WHEN label ~ '^[0-9]+-ply'
        THEN (substring(label FROM '^([0-9]+)')::integer + 1)::text
             || substring(label FROM '^[0-9]+(-ply.*)$')
        ELSE label END
$$;

-- Increment a numeric key of one object.
CREATE FUNCTION pg_temp.shift_key(item jsonb, key text) RETURNS jsonb
LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE WHEN jsonb_typeof(item -> key) = 'number'
        THEN jsonb_set(item, ARRAY[key], to_jsonb((item ->> key)::integer + 1))
        ELSE item END
$$;

-- Increment `plies` of every native candidate in the array under `key`;
-- objects without that array stay unchanged.
CREATE FUNCTION pg_temp.shift_candidates(item jsonb, key text) RETURNS jsonb
LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE WHEN jsonb_typeof(item -> key) = 'array' THEN item || jsonb_build_object(key, (
        SELECT coalesce(jsonb_agg(pg_temp.shift_key(c, 'plies') ORDER BY o), '[]'::jsonb)
        FROM jsonb_array_elements(item -> key) WITH ORDINALITY AS e(c, o)
    )) ELSE item END
$$;

-- Depth and label of one imported record or candidate.
CREATE FUNCTION pg_temp.shift_imported_depth(item jsonb) RETURNS jsonb
LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE WHEN jsonb_typeof(item -> 'depth_plies') = 'number'
        THEN pg_temp.shift_key(item, 'depth_plies')
             || CASE WHEN jsonb_typeof(item -> 'depth') = 'string'
                THEN jsonb_build_object('depth', pg_temp.shift_label(item ->> 'depth'))
                ELSE '{}'::jsonb END
        ELSE item END
$$;

-- An imported record from GNUbg's own SGF analysis properties (A / DA).
CREATE FUNCTION pg_temp.shift_imported_record(item jsonb) RETURNS jsonb
LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE WHEN item -> 'raw' ?| ARRAY['A', 'DA'] THEN
        pg_temp.shift_imported_depth(item)
        || CASE WHEN jsonb_typeof(item -> 'candidates') = 'array' THEN jsonb_build_object(
            'candidates', (
                SELECT coalesce(jsonb_agg(pg_temp.shift_imported_depth(c) ORDER BY o), '[]'::jsonb)
                FROM jsonb_array_elements(item -> 'candidates') WITH ORDINALITY AS e(c, o)))
           ELSE '{}'::jsonb END
    ELSE item END
$$;

-- "<engine>; N-ply cubeful deterministic"
CREATE FUNCTION pg_temp.shift_rating(item jsonb) RETURNS jsonb
LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE WHEN jsonb_typeof(item #> '{performance_rating,profile}') = 'string'
                 AND item #>> '{performance_rating,profile}' ~ '; [0-9]+-ply cubeful deterministic$'
        THEN jsonb_set(item, '{performance_rating,profile}', to_jsonb(
            regexp_replace(item #>> '{performance_rating,profile}', '; [0-9]+-ply cubeful deterministic$', '')
            || '; ' || pg_temp.shift_label(substring(
                item #>> '{performance_rating,profile}' FROM '; ([0-9]+-ply cubeful deterministic)$'))))
        ELSE item END
$$;

-- Saved settings and profiles.
ALTER TABLE app_settings
    DROP CONSTRAINT app_settings_analysis_plies_check,
    DROP CONSTRAINT app_settings_play_plies_check;
UPDATE app_settings SET
    analysis_plies = analysis_plies + CASE WHEN analysis_engine = 'gnubg' THEN 1 ELSE 0 END,
    play_plies = play_plies + CASE WHEN play_engine = 'gnubg' THEN 1 ELSE 0 END,
    version = version + 1
WHERE analysis_engine = 'gnubg' OR play_engine = 'gnubg';
ALTER TABLE app_settings
    ALTER COLUMN analysis_plies SET DEFAULT 3,
    ALTER COLUMN play_plies SET DEFAULT 1,
    ADD CONSTRAINT app_settings_analysis_plies_check CHECK (analysis_plies BETWEEN 1 AND 5),
    ADD CONSTRAINT app_settings_play_plies_check CHECK (play_plies BETWEEN 1 AND 3);

CREATE FUNCTION pg_temp.shift_configuration(c jsonb) RETURNS jsonb
LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE WHEN c #>> '{play,engine}' = 'gnubg' AND jsonb_typeof(c #> '{play,plies}') = 'number'
        THEN jsonb_set(x, '{play,plies}', to_jsonb((c #>> '{play,plies}')::integer + 1))
        ELSE x END
    FROM (SELECT CASE WHEN c #>> '{analysis,engine}' = 'gnubg' AND jsonb_typeof(c #> '{analysis,plies}') = 'number'
        THEN jsonb_set(c, '{analysis,plies}', to_jsonb((c #>> '{analysis,plies}')::integer + 1))
        ELSE c END AS x) shifted
$$;
UPDATE configuration_profiles
SET configuration = pg_temp.shift_configuration(configuration), revision = revision + 1
WHERE configuration <> pg_temp.shift_configuration(configuration);
UPDATE configuration_state
SET current_configuration = pg_temp.shift_configuration(current_configuration), revision = revision + 1
WHERE current_configuration <> pg_temp.shift_configuration(current_configuration);

-- Native jobs and their checkpoints. Jobs without a snapshot predate Open Sage.
CREATE TEMP TABLE gnubg_jobs ON COMMIT DROP AS
SELECT id FROM jobs WHERE coalesce(engine_snapshot ->> 'engine_id', 'gnubg') = 'gnubg';
ALTER TABLE jobs DROP CONSTRAINT jobs_plies_check;
-- Two steps keep the active-job unique indexes free of transient collisions.
UPDATE jobs SET plies = plies + 101 WHERE id IN (SELECT id FROM gnubg_jobs);
UPDATE jobs SET
    plies = plies - 100,
    result = CASE
        WHEN result IS NULL OR jsonb_typeof(result) <> 'object' THEN result
        WHEN kind = 'analysis' THEN pg_temp.shift_candidates(pg_temp.shift_key(result, 'plies'), 'candidates')
        WHEN kind = 'dice' THEN pg_temp.shift_key(result, 'plies')
            || jsonb_build_object('rolls', (
                SELECT coalesce(jsonb_agg(CASE WHEN jsonb_typeof(r -> 'best') = 'object'
                    THEN jsonb_set(r, '{best}', pg_temp.shift_key(r -> 'best', 'plies')) ELSE r END
                    ORDER BY o), '[]'::jsonb)
                FROM jsonb_array_elements(CASE WHEN jsonb_typeof(result -> 'rolls') = 'array'
                    THEN result -> 'rolls' ELSE '[]'::jsonb END) WITH ORDINALITY AS e(r, o)))
        ELSE pg_temp.shift_rating(pg_temp.shift_key(result, 'plies'))
    END
WHERE id IN (SELECT id FROM gnubg_jobs);
ALTER TABLE jobs ADD CONSTRAINT jobs_plies_check CHECK (plies BETWEEN 1 AND 5);

UPDATE match_analysis_games
SET summary = pg_temp.shift_rating(pg_temp.shift_key(summary, 'plies'))
WHERE job_id IN (SELECT id FROM gnubg_jobs) AND jsonb_typeof(summary) = 'object';

UPDATE move_analyses
SET data = pg_temp.shift_candidates(data, 'checker_candidates')
WHERE job_id IN (SELECT id FROM gnubg_jobs) AND jsonb_typeof(data -> 'checker_candidates') = 'array';

-- Benchmarks. Protocol ids keep naming GNUbg's native level.
ALTER TABLE benchmark_runs
    DROP CONSTRAINT benchmark_runs_plies_check,
    DROP CONSTRAINT benchmark_workload;
UPDATE benchmark_runs SET plies = plies + 1 WHERE engine_id = 'gnubg';
ALTER TABLE benchmark_runs
    ADD CONSTRAINT benchmark_runs_plies_check CHECK (plies BETWEEN 1 AND 5),
    ADD CONSTRAINT benchmark_workload CHECK (
        (play_reference IS NULL AND reference IS NOT NULL AND protocol IS NOT NULL
            AND reference_sgf IS NOT NULL AND completed_turns = 0 AND average_turn_ms IS NULL)
        OR (play_reference IS NOT NULL AND reference IS NULL AND protocol IS NULL
            AND reference_sgf IS NULL AND plies BETWEEN 1 AND 3 AND completed_games = 0
            AND analyzed_moves = 0 AND average_move_ms IS NULL
            AND (play_reference ->> 'total_turns')::integer IS NOT NULL
            AND completed_turns BETWEEN 0 AND (play_reference ->> 'total_turns')::integer));

UPDATE score_practice_profiles SET plies = plies + 1 WHERE engine ->> 'engine_id' = 'gnubg';

-- Engine duels: the saved configuration drives labels, validation and resumes.
UPDATE engine_match_runs SET configuration = configuration
    || CASE WHEN configuration #>> '{a,engine_id}' = 'gnubg'
        THEN jsonb_build_object('a', pg_temp.shift_key(configuration -> 'a', 'plies')) ELSE '{}'::jsonb END
    || CASE WHEN configuration #>> '{b,engine_id}' = 'gnubg'
        THEN jsonb_build_object('b', pg_temp.shift_key(configuration -> 'b', 'plies')) ELSE '{}'::jsonb END
WHERE 'gnubg' IN (configuration #>> '{a,engine_id}', configuration #>> '{b,engine_id}');

-- Play sessions: the bot depth and the GNUbg feedback depth.
UPDATE play_sessions SET state = state
    || CASE WHEN state #>> '{engine,id}' = 'gnubg'
        THEN jsonb_build_object('engine', pg_temp.shift_key(state -> 'engine', 'plies')) ELSE '{}'::jsonb END
    || CASE WHEN coalesce(state #>> '{engine_snapshots,analysis,engine_id}', 'gnubg') = 'gnubg'
        THEN jsonb_build_object('analysis_plies', coalesce((state ->> 'analysis_plies')::integer, 2) + 1)
        ELSE '{}'::jsonb END,
    version = version + 1
WHERE state #>> '{engine,id}' = 'gnubg'
   OR coalesce(state #>> '{engine_snapshots,analysis,engine_id}', 'gnubg') = 'gnubg';

-- Imported GNUbg SGF evaluations record GNUbg's native level in their raw
-- tokens; their interpreted depth follows BG Tutor's convention.
CREATE TEMP TABLE gnubg_imports ON COMMIT DROP AS
SELECT id FROM imported_analyses
WHERE data -> 'raw' ?| ARRAY['A', 'DA'] AND data <> pg_temp.shift_imported_record(data);
UPDATE imported_analyses SET data = pg_temp.shift_imported_record(data)
WHERE id IN (SELECT id FROM gnubg_imports);
-- Derived cache; rebuilt on the next read.
DELETE FROM imported_analysis_projections
WHERE imported_analysis_id IN (SELECT id FROM gnubg_imports);

-- Saved cards keep their solution snapshot; shift its GNUbg depths.
UPDATE learning_cards SET payload = jsonb_set(payload, '{solution}',
    CASE WHEN jsonb_typeof(payload #> '{solution,imported}') = 'object' THEN
        CASE WHEN payload #> '{solution,imported,raw}' ?| ARRAY['A', 'DA']
            THEN jsonb_set(pg_temp.shift_key(payload -> 'solution', 'plies'), '{imported}',
                pg_temp.shift_imported_record(payload #> '{solution,imported}'))
            ELSE payload -> 'solution' END
    ELSE pg_temp.shift_candidates(pg_temp.shift_key(payload -> 'solution', 'plies'), 'candidates')
        || CASE WHEN jsonb_typeof(payload #> '{solution,decision}') = 'object'
            THEN jsonb_build_object('decision',
                pg_temp.shift_candidates(payload #> '{solution,decision}', 'checker_candidates'))
            ELSE '{}'::jsonb END
    END)
WHERE jsonb_typeof(payload -> 'solution') = 'object'
  AND payload #>> '{solution,engine}' LIKE 'GNU Backgammon%';

DROP FUNCTION pg_temp.shift_configuration(jsonb);
DROP FUNCTION pg_temp.shift_rating(jsonb);
DROP FUNCTION pg_temp.shift_imported_record(jsonb);
DROP FUNCTION pg_temp.shift_imported_depth(jsonb);
DROP FUNCTION pg_temp.shift_candidates(jsonb, text);
DROP FUNCTION pg_temp.shift_key(jsonb, text);
DROP FUNCTION pg_temp.shift_label(text);
