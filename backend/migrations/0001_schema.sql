-- NeonDojo database schema.
--
-- This baseline consolidates the former migrations 0001-0084. It is the
-- schema those migrations produced, generated with pg_dump from a freshly
-- migrated PostgreSQL 17 database, followed by the rows they seeded.
-- Databases migrated with the former files are adopted at startup by
-- backend/src/schema_migrations.rs; add later schema changes as new
-- migrations after this file instead of editing it.

-- Function bodies may reference tables created further down.
SET LOCAL check_function_bodies = false;

--
-- Name: pg_trgm; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;


--
-- Name: annotations_search_text(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.annotations_search_text() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.search_text := lower(NEW.comment || ' ' || array_to_string(NEW.tags, ' '));
    RETURN NEW;
END
$$;


--
-- Name: catalog_match_players(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.catalog_match_players() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    site text;
    participant record;
BEGIN
    DELETE FROM match_players WHERE match_id=NEW.id;
    IF NEW.collection_only OR NEW.play_mode IS NOT NULL
       OR NEW.database_id='00000000-0000-0000-0000-000000000005' THEN
        RETURN NEW;
    END IF;

    site := coalesce(NEW.player_source_platform, '');

    -- Use the same lock order when two concurrent imports share both aliases.
    FOR participant IN
        SELECT * FROM (VALUES (0, btrim(NEW.white)), (1, btrim(NEW.black))) AS names(side, name)
        WHERE name<>'' ORDER BY name COLLATE "C", side
    LOOP
        INSERT INTO match_players(match_id, side, alias_id)
        VALUES(NEW.id, participant.side, resolve_imported_player(site, participant.name));
    END LOOP;
    RETURN NEW;
END;
$$;


--
-- Name: decision_index_drop_updated_analyses(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.decision_index_drop_updated_analyses() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    DELETE FROM decision_index_matches x
    USING jobs j
    WHERE x.match_id = j.match_uuid
      AND j.id IN (SELECT job_id FROM old_rows UNION SELECT job_id FROM new_rows);
    RETURN NULL;
END
$$;


--
-- Name: imported_analysis_has_values(jsonb); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.imported_analysis_has_values(data jsonb) RETURNS boolean
    LANGUAGE sql IMMUTABLE PARALLEL SAFE
    AS $_$
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
$_$;


--
-- Name: position_browser_bits(bytea, integer, integer); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.position_browser_bits(data bytea, bit_offset integer, bit_width integer) RETURNS integer
    LANGUAGE plpgsql IMMUTABLE STRICT PARALLEL SAFE
    AS $$
DECLARE value integer := 0; n integer;
BEGIN
    FOR n IN 0..bit_width-1 LOOP
        value := value | (((get_byte(data,(bit_offset+n)/8) >> ((bit_offset+n)%8)) & 1) << n);
    END LOOP;
    RETURN value;
END $$;


--
-- Name: position_browser_context(text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.position_browser_context(match_id text) RETURNS integer[]
    LANGUAGE plpgsql IMMUTABLE STRICT PARALLEL SAFE
    SET search_path TO 'pg_catalog', 'public'
    AS $_$
DECLARE data bytea; owner integer;
BEGIN
    IF match_id !~ '^[A-Za-z0-9+/]{12}$' THEN RETURN NULL; END IF;
    data := decode(match_id,'base64');
    IF octet_length(data) <> 9
       OR position_browser_bits(data,8,3) <> 1
       OR position_browser_bits(data,15,3) NOT BETWEEN 1 AND 6
       OR position_browser_bits(data,18,3) NOT BETWEEN 1 AND 6 THEN
        RETURN NULL;
    END IF;
    owner := position_browser_bits(data,4,2);
    IF owner = 2 THEN RETURN NULL; END IF;
    RETURN ARRAY[position_browser_bits(data,36,15),position_browser_bits(data,51,15),
        position_browser_bits(data,7,1),1 << position_browser_bits(data,0,4),
        CASE WHEN owner=3 THEN -1 ELSE owner END];
EXCEPTION WHEN OTHERS THEN RETURN NULL;
END $_$;


--
-- Name: protect_system_database(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.protect_system_database() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'System databases cannot be deleted' USING ERRCODE = '23514';
    END IF;
    IF NEW.id <> OLD.id OR NEW.is_default <> OLD.is_default THEN
        RAISE EXCEPTION 'System database identities cannot be changed' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END $$;


--
-- Name: resolve_imported_player(text, text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.resolve_imported_player(site text, alias_name text) RETURNS uuid
    LANGUAGE plpgsql
    AS $$
DECLARE
    alias_uuid uuid;
    profile_uuid uuid;
BEGIN
    SELECT id INTO alias_uuid FROM player_aliases WHERE platform=site AND name=alias_name;
    IF FOUND THEN RETURN alias_uuid; END IF;

    INSERT INTO player_profiles(source_name) VALUES(alias_name) RETURNING id INTO profile_uuid;
    INSERT INTO player_aliases(platform, name, profile_id) VALUES(site, alias_name, profile_uuid)
        ON CONFLICT(platform, name) DO NOTHING RETURNING id INTO alias_uuid;
    IF NOT FOUND THEN
        DELETE FROM player_profiles WHERE id=profile_uuid;
        SELECT id INTO STRICT alias_uuid FROM player_aliases WHERE platform=site AND name=alias_name;
    END IF;
    RETURN alias_uuid;
END;
$$;


--
-- Name: review_hits_drop_updated_imported(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.review_hits_drop_updated_imported() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    DELETE FROM review_hit_sets x
    WHERE x.match_id IN (SELECT match_uuid FROM old_rows UNION SELECT match_uuid FROM new_rows);
    RETURN NULL;
END
$$;


--
-- Name: review_hits_drop_updated_native(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.review_hits_drop_updated_native() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    DELETE FROM review_hit_sets x
    USING jobs j
    WHERE x.match_id = j.match_uuid
      AND j.id IN (SELECT job_id FROM old_rows UNION SELECT job_id FROM new_rows);
    RETURN NULL;
END
$$;


--
-- Name: set_match_player_platform(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.set_match_player_platform() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.player_source_platform := coalesce(NEW.player_source_platform, CASE
        WHEN lower(btrim(coalesce(NEW.place, ''))) IN ('backgammongalaxy', 'backgammongalaxy.com')
          OR lower(btrim(coalesce(NEW.tournament, ''))) IN ('backgammongalaxy.com', 'https://www.backgammongalaxy.com/')
        THEN 'backgammongalaxy.com'
        WHEN lower(btrim(coalesce(NEW.place, ''))) IN ('opengammon', 'opengammon.com')
          OR lower(btrim(coalesce(NEW.tournament, ''))) IN ('opengammon.com', 'https://opengammon.com/')
        THEN 'opengammon.com'
        ELSE ''
    END);
    RETURN NEW;
END;
$$;


--
-- Name: snapshot_job_source_revision(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.snapshot_job_source_revision() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
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


--
-- Name: imported_analyses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.imported_analyses (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    match_uuid uuid NOT NULL,
    position_uuid uuid,
    source_ordinal integer NOT NULL,
    game_no integer NOT NULL,
    source_move_no integer NOT NULL,
    kind text NOT NULL,
    data jsonb NOT NULL,
    CONSTRAINT imported_analyses_source_move_no_check CHECK ((source_move_no >= 0)),
    CONSTRAINT imported_analyses_source_ordinal_check CHECK ((source_ordinal >= 0))
);


--
-- Name: jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.jobs (
    id uuid NOT NULL,
    position_uuid uuid,
    kind text NOT NULL,
    plies integer NOT NULL,
    status text NOT NULL,
    progress integer DEFAULT 0 NOT NULL,
    result jsonb,
    error text,
    attempts integer DEFAULT 0 NOT NULL,
    lease_token uuid,
    lease_until timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    match_uuid uuid,
    total_games integer DEFAULT 0 NOT NULL,
    completed_games integer DEFAULT 0 NOT NULL,
    game_no integer,
    engine_config text DEFAULT 'legacy'::text NOT NULL,
    engine_snapshot jsonb,
    source_revision bigint DEFAULT '-1'::integer NOT NULL,
    CONSTRAINT jobs_check CHECK (((completed_games >= 0) AND (completed_games <= total_games))),
    CONSTRAINT jobs_game_scope_check CHECK (((game_no IS NULL) OR ((kind = 'match'::text) AND (game_no > 0) AND (total_games = 1)))),
    CONSTRAINT jobs_kind_check CHECK ((kind = ANY (ARRAY['analysis'::text, 'dice'::text, 'match'::text]))),
    CONSTRAINT jobs_plies_check CHECK (((plies >= 1) AND (plies <= 5))),
    CONSTRAINT jobs_progress_check CHECK (((progress >= 0) AND (progress <= 100))),
    CONSTRAINT jobs_source_revision_nonnegative CHECK ((source_revision >= 0)),
    CONSTRAINT jobs_status_check CHECK ((status = ANY (ARRAY['queued'::text, 'running'::text, 'completed'::text, 'failed'::text]))),
    CONSTRAINT jobs_target_check CHECK ((((kind = 'match'::text) AND (match_uuid IS NOT NULL) AND (position_uuid IS NULL)) OR ((kind = ANY (ARRAY['analysis'::text, 'dice'::text])) AND (position_uuid IS NOT NULL) AND (match_uuid IS NULL)))),
    CONSTRAINT jobs_total_games_check CHECK ((total_games >= 0))
);


--
-- Name: match_analysis_games; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.match_analysis_games (
    job_id uuid NOT NULL,
    game_no integer NOT NULL,
    summary jsonb,
    CONSTRAINT match_analysis_games_game_no_check CHECK ((game_no > 0))
);


--
-- Name: matches; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.matches (
    id uuid NOT NULL,
    filename text NOT NULL,
    format text NOT NULL,
    sha256 text NOT NULL,
    white text NOT NULL,
    black text NOT NULL,
    match_length integer NOT NULL,
    position_count integer NOT NULL,
    original bytea NOT NULL,
    sgf text NOT NULL,
    search_text text GENERATED ALWAYS AS (lower(((((white || ' '::text) || black) || ' '::text) || filename))) STORED,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    database_id uuid DEFAULT '81a18d0f-38c1-4db8-81c0-a743adbc4480'::uuid NOT NULL,
    display_name text DEFAULT ''::text NOT NULL,
    played_on date,
    tournament text,
    round text,
    place text,
    result_state text DEFAULT 'unknown'::text NOT NULL,
    white_score integer,
    black_score integer,
    metadata_version bigint DEFAULT 0 NOT NULL,
    source_metadata_loaded boolean DEFAULT false NOT NULL,
    moves_cataloged boolean DEFAULT false NOT NULL,
    imported_analysis_loaded boolean DEFAULT false NOT NULL,
    match_no bigint NOT NULL,
    collection_only boolean DEFAULT false NOT NULL,
    source_match_id uuid,
    source_match_no bigint,
    source_database_id uuid,
    source_database_name text,
    source_revision bigint DEFAULT 0 NOT NULL,
    play_mode text,
    play_assisted boolean DEFAULT false NOT NULL,
    native_match_context_pending boolean DEFAULT false NOT NULL,
    player_source_platform text,
    CONSTRAINT matches_black_score_check CHECK ((black_score >= 0)),
    CONSTRAINT matches_collection_source CHECK (((collection_only = (database_id = '00000000-0000-0000-0000-000000000002'::uuid)) AND ((NOT collection_only) OR ((source_match_id IS NOT NULL) AND (source_match_no > 0) AND (source_database_id IS NOT NULL) AND (source_database_name IS NOT NULL) AND (octet_length(original) = 0) AND (octet_length(sgf) = 0) AND moves_cataloged AND imported_analysis_loaded AND source_metadata_loaded)))),
    CONSTRAINT matches_display_name_check CHECK ((char_length(display_name) <= 240)),
    CONSTRAINT matches_format_check CHECK ((format = ANY (ARRAY['mat'::text, 'sgf'::text, 'xg'::text, 'bgf'::text, 'ogxm'::text]))),
    CONSTRAINT matches_match_length_check CHECK ((match_length >= 0)),
    CONSTRAINT matches_metadata_version_check CHECK ((metadata_version >= 0)),
    CONSTRAINT matches_place_check CHECK ((char_length(place) <= 240)),
    CONSTRAINT matches_player_source_platform_check CHECK ((player_source_platform = ANY (ARRAY[''::text, 'backgammongalaxy.com'::text, 'opengammon.com'::text]))),
    CONSTRAINT matches_result_state_check CHECK ((result_state = ANY (ARRAY['unknown'::text, 'ongoing'::text, 'completed'::text]))),
    CONSTRAINT matches_round_check CHECK ((char_length(round) <= 120)),
    CONSTRAINT matches_tournament_check CHECK ((char_length(tournament) <= 240)),
    CONSTRAINT matches_white_score_check CHECK ((white_score >= 0))
);


--
-- Name: move_analyses; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.move_analyses (
    id bigint NOT NULL,
    job_id uuid NOT NULL,
    position_uuid uuid,
    game_no integer NOT NULL,
    decision_no integer NOT NULL,
    player integer NOT NULL,
    kind text NOT NULL,
    equity_loss double precision NOT NULL,
    data jsonb NOT NULL,
    CONSTRAINT move_analyses_decision_no_check CHECK ((decision_no > 0)),
    CONSTRAINT move_analyses_equity_loss_check CHECK (((equity_loss >= (0)::double precision) AND (equity_loss < 'Infinity'::double precision))),
    CONSTRAINT move_analyses_game_no_check CHECK ((game_no > 0)),
    CONSTRAINT move_analyses_kind_check CHECK ((kind = ANY (ARRAY['checker'::text, 'cube'::text]))),
    CONSTRAINT move_analyses_player_check CHECK ((player = ANY (ARRAY[0, 1])))
);


--
-- Name: play_sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.play_sessions (
    id uuid NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    state jsonb NOT NULL,
    gnubg_path text NOT NULL,
    sgf text NOT NULL,
    native_id text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    library_match_id uuid,
    library_archived boolean DEFAULT false NOT NULL,
    paused_from_database_id uuid,
    CONSTRAINT play_sessions_version_check CHECK ((version >= 0))
);


--
-- Name: positions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.positions (
    id uuid NOT NULL,
    match_uuid uuid NOT NULL,
    ordinal integer NOT NULL,
    game_no integer NOT NULL,
    ply integer NOT NULL,
    player integer NOT NULL,
    dice integer[] NOT NULL,
    board integer[] NOT NULL,
    position_id text NOT NULL,
    gnubg_match_id text NOT NULL,
    notation text NOT NULL,
    source_white_score integer GENERATED ALWAYS AS ((public.position_browser_context(gnubg_match_id))[1]) STORED,
    source_black_score integer GENERATED ALWAYS AS ((public.position_browser_context(gnubg_match_id))[2]) STORED,
    source_crawford boolean GENERATED ALWAYS AS (((public.position_browser_context(gnubg_match_id))[3] = 1)) STORED,
    source_cube_value integer GENERATED ALWAYS AS ((public.position_browser_context(gnubg_match_id))[4]) STORED,
    source_cube_owner integer GENERATED ALWAYS AS ((public.position_browser_context(gnubg_match_id))[5]) STORED,
    CONSTRAINT positions_board_check CHECK ((array_length(board, 1) = 26)),
    CONSTRAINT positions_dice_check CHECK ((array_length(dice, 1) = 2)),
    CONSTRAINT positions_player_check CHECK ((player = ANY (ARRAY[0, 1])))
);


--
-- Name: primary_imported_analyses; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.primary_imported_analyses AS
 SELECT DISTINCT ON (match_uuid, position_uuid, game_no, source_move_no, kind) id,
    match_uuid,
    position_uuid,
    source_ordinal,
    game_no,
    source_move_no,
    kind,
    data
   FROM public.imported_analyses
  ORDER BY match_uuid, position_uuid, game_no, source_move_no, kind, (public.imported_analysis_has_values(data)) DESC, source_ordinal, id;


--
-- Name: analysis_catalog_sources; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.analysis_catalog_sources AS
 SELECT m.id AS match_id,
    ('job:'::text || (j.id)::text) AS analysis_id,
    'native'::text AS source,
        CASE
            WHEN (j.game_no IS NULL) THEN 'match'::text
            ELSE 'game'::text
        END AS scope,
    j.id AS job_id,
    j.created_at,
    j.status,
    ((j.status = 'completed'::text) AND (j.completed_games = j.total_games) AND (checkpoints.games >= j.total_games) AND (NOT m.collection_only)) AS complete,
    counts.checker_count,
    counts.cube_count,
    COALESCE(NULLIF((j.result ->> 'engine'::text), ''::text),
        CASE
            WHEN ((j.engine_snapshot ->> 'engine_id'::text) = 'bgsage'::text) THEN 'NeonGammon'::text
            ELSE 'GNU Backgammon'::text
        END) AS engine,
    ((j.plies)::text || '-ply cubeful'::text) AS depth,
    j.game_no,
    NULL::uuid AS position_id
   FROM (((public.matches m
     JOIN public.jobs j ON (((j.match_uuid = m.id) AND (j.source_revision = m.source_revision) AND (j.kind = 'match'::text))))
     JOIN LATERAL ( SELECT count(*) AS games
           FROM public.match_analysis_games c
          WHERE ((c.job_id = j.id) AND ((j.game_no IS NULL) OR (c.game_no = j.game_no)))) checkpoints ON ((checkpoints.games > 0)))
     LEFT JOIN LATERAL ( SELECT count(*) FILTER (WHERE (a.kind = 'checker'::text)) AS checker_count,
            count(*) FILTER (WHERE (a.kind = 'cube'::text)) AS cube_count
           FROM (public.move_analyses a
             JOIN public.match_analysis_games c ON (((c.job_id = a.job_id) AND (c.game_no = a.game_no))))
          WHERE ((a.job_id = j.id) AND ((j.game_no IS NULL) OR (a.game_no = j.game_no)))) counts ON (true))
  WHERE ((j.status = ANY (ARRAY['completed'::text, 'running'::text, 'failed'::text])) AND ((EXISTS ( SELECT 1
           FROM (public.move_analyses a
             JOIN public.match_analysis_games c ON (((c.job_id = a.job_id) AND (c.game_no = a.game_no))))
          WHERE ((a.job_id = j.id) AND ((j.game_no IS NULL) OR (a.game_no = j.game_no))))) OR (EXISTS ( SELECT 1
           FROM public.match_analysis_games c
          WHERE ((c.job_id = j.id) AND ((j.game_no IS NULL) OR (c.game_no = j.game_no)) AND ((c.summary ->> 'completed_games'::text) = '1'::text) AND ((c.summary ->> 'total_games'::text) = '1'::text))))) AND (NOT (EXISTS ( SELECT 1
           FROM public.play_sessions live
          WHERE ((live.library_match_id = m.id) AND (COALESCE((live.state ->> 'play_mode'::text), 'competition'::text) = 'competition'::text) AND ((live.state ->> 'phase'::text) <> 'match_over'::text) AND (NOT (((live.state ->> 'match_length'::text) = '0'::text) AND ((live.state ->> 'phase'::text) = 'game_over'::text))))))))
UNION ALL
 SELECT m.id AS match_id,
    ('job:'::text || (j.id)::text) AS analysis_id,
    'native'::text AS source,
    'position'::text AS scope,
    j.id AS job_id,
    j.created_at,
    j.status,
    true AS complete,
    (1)::bigint AS checker_count,
    (0)::bigint AS cube_count,
    COALESCE(NULLIF((j.result ->> 'engine'::text), ''::text),
        CASE
            WHEN ((j.engine_snapshot ->> 'engine_id'::text) = 'bgsage'::text) THEN 'NeonGammon'::text
            ELSE 'GNU Backgammon'::text
        END) AS engine,
    ((j.plies)::text || '-ply'::text) AS depth,
    p.game_no,
    p.id AS position_id
   FROM ((public.matches m
     JOIN public.positions p ON ((p.match_uuid = m.id)))
     JOIN public.jobs j ON (((j.position_uuid = p.id) AND (j.kind = 'analysis'::text) AND (j.source_revision = m.source_revision))))
  WHERE ((j.status = 'completed'::text) AND (jsonb_typeof((j.result -> 'candidates'::text)) = 'array'::text) AND (jsonb_array_length(
        CASE
            WHEN (jsonb_typeof((j.result -> 'candidates'::text)) = 'array'::text) THEN (j.result -> 'candidates'::text)
            ELSE '[]'::jsonb
        END) > 0) AND (NOT (EXISTS ( SELECT 1
           FROM public.play_sessions live
          WHERE ((live.library_match_id = m.id) AND (COALESCE((live.state ->> 'play_mode'::text), 'competition'::text) = 'competition'::text) AND ((live.state ->> 'phase'::text) <> 'match_over'::text) AND (NOT (((live.state ->> 'match_length'::text) = '0'::text) AND ((live.state ->> 'phase'::text) = 'game_over'::text))))))))
UNION ALL
 SELECT m.id AS match_id,
    ('imported:'::text || (m.id)::text) AS analysis_id,
    'imported'::text AS source,
    'match'::text AS scope,
    NULL::uuid AS job_id,
    m.created_at,
    'completed'::text AS status,
    false AS complete,
    imported.checker_count,
    imported.cube_count,
    imported.engine,
    imported.depth,
    NULL::integer AS game_no,
    NULL::uuid AS position_id
   FROM (public.matches m
     LEFT JOIN LATERAL ( SELECT count(*) FILTER (WHERE (a.kind = 'checker'::text)) AS checker_count,
            count(*) FILTER (WHERE (a.kind = 'cube'::text)) AS cube_count,
            COALESCE(string_agg(DISTINCT NULLIF(concat_ws(' '::text, (a.data ->> 'engine'::text), (a.data ->> 'engine_version'::text)), ''::text), ', '::text ORDER BY NULLIF(concat_ws(' '::text, (a.data ->> 'engine'::text), (a.data ->> 'engine_version'::text)), ''::text)), 'Imported'::text) AS engine,
            string_agg(DISTINCT (a.data ->> 'depth'::text), ', '::text ORDER BY (a.data ->> 'depth'::text)) AS depth
           FROM public.primary_imported_analyses a
          WHERE ((a.match_uuid = m.id) AND (a.kind = ANY (ARRAY['checker'::text, 'cube'::text])) AND public.imported_analysis_has_values(a.data))) imported ON (true))
  WHERE ((EXISTS ( SELECT 1
           FROM public.imported_analyses a
          WHERE ((a.match_uuid = m.id) AND (a.kind = ANY (ARRAY['checker'::text, 'cube'::text])) AND public.imported_analysis_has_values(a.data)))) AND (NOT (EXISTS ( SELECT 1
           FROM public.play_sessions live
          WHERE ((live.library_match_id = m.id) AND (COALESCE((live.state ->> 'play_mode'::text), 'competition'::text) = 'competition'::text) AND ((live.state ->> 'phase'::text) <> 'match_over'::text) AND (NOT (((live.state ->> 'match_length'::text) = '0'::text) AND ((live.state ->> 'phase'::text) = 'game_over'::text))))))));


--
-- Name: analysis_selections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.analysis_selections (
    match_id uuid NOT NULL,
    source_revision bigint DEFAULT 0 NOT NULL,
    analysis_id text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT analysis_selections_analysis_id_check CHECK ((analysis_id ~ '^(job|imported):[0-9a-f-]{36}$'::text)),
    CONSTRAINT analysis_selections_source_revision_check CHECK ((source_revision >= 0))
);


--
-- Name: anki_auto_sync; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.anki_auto_sync (
    deck_id uuid NOT NULL,
    link_id uuid NOT NULL,
    profile text NOT NULL,
    collection_key text NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    last_synced_at timestamp with time zone,
    last_received_at timestamp with time zone,
    last_outcome text,
    received_reviews bigint DEFAULT 0 NOT NULL,
    sent_reviews bigint DEFAULT 0 NOT NULL,
    last_problem text,
    paused boolean DEFAULT false NOT NULL,
    running_until timestamp with time zone,
    CONSTRAINT anki_auto_sync_last_outcome_check CHECK ((last_outcome = ANY (ARRAY['synced'::text, 'no_matches'::text, 'failed'::text]))),
    CONSTRAINT anki_auto_sync_last_problem_check CHECK ((last_problem = ANY (ARRAY['unreachable'::text, 'invalid_deck'::text, 'conflict'::text, 'timeout'::text, 'anki_error'::text, 'internal_error'::text]))),
    CONSTRAINT anki_auto_sync_received_reviews_check CHECK ((received_reviews >= 0)),
    CONSTRAINT anki_auto_sync_sent_reviews_check CHECK ((sent_reviews >= 0))
);


--
-- Name: anki_card_faces; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.anki_card_faces (
    card_id uuid NOT NULL,
    package text NOT NULL,
    css text NOT NULL,
    front text NOT NULL,
    back text NOT NULL,
    template_number integer NOT NULL,
    CONSTRAINT anki_card_faces_template_number_check CHECK ((template_number > 0))
);


--
-- Name: anki_media; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.anki_media (
    package text NOT NULL,
    name text NOT NULL,
    content_type text NOT NULL,
    content bytea NOT NULL
);


--
-- Name: anki_received_reviews; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.anki_received_reviews (
    card_id uuid NOT NULL,
    review_id bigint NOT NULL,
    rating text NOT NULL,
    CONSTRAINT anki_received_reviews_rating_check CHECK ((rating = ANY (ARRAY['again'::text, 'hard'::text, 'good'::text, 'easy'::text]))),
    CONSTRAINT anki_received_reviews_review_id_check CHECK ((review_id > 0))
);


--
-- Name: anki_review_exports; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.anki_review_exports (
    card_id uuid NOT NULL,
    request_id uuid NOT NULL,
    review_id bigint NOT NULL,
    CONSTRAINT anki_review_exports_review_id_check CHECK ((review_id > 0))
);


--
-- Name: anki_source_decks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.anki_source_decks (
    source_name text NOT NULL,
    deck_id uuid NOT NULL
);


--
-- Name: anki_sync_history; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.anki_sync_history (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    deck_id uuid NOT NULL,
    profile text NOT NULL,
    finished_at timestamp with time zone DEFAULT now() NOT NULL,
    received_reviews bigint NOT NULL,
    sent_reviews bigint NOT NULL,
    outcome text NOT NULL,
    problem text,
    error_detail text,
    resolved_at timestamp with time zone,
    CONSTRAINT anki_sync_history_check CHECK (((outcome = 'failed'::text) = ((problem IS NOT NULL) AND (error_detail IS NOT NULL)))),
    CONSTRAINT anki_sync_history_check1 CHECK (((resolved_at IS NULL) OR ((outcome = 'failed'::text) AND (resolved_at >= finished_at)))),
    CONSTRAINT anki_sync_history_error_detail_check CHECK ((char_length(error_detail) <= 300)),
    CONSTRAINT anki_sync_history_outcome_check CHECK ((outcome = ANY (ARRAY['synced'::text, 'no_matches'::text, 'failed'::text]))),
    CONSTRAINT anki_sync_history_problem_check CHECK ((problem = ANY (ARRAY['unreachable'::text, 'invalid_deck'::text, 'conflict'::text, 'timeout'::text, 'anki_error'::text, 'internal_error'::text]))),
    CONSTRAINT anki_sync_history_received_reviews_check CHECK ((received_reviews >= 0)),
    CONSTRAINT anki_sync_history_sent_reviews_check CHECK ((sent_reviews >= 0))
);


--
-- Name: annotations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.annotations (
    id uuid NOT NULL,
    match_uuid uuid,
    position_uuid uuid,
    tags text[] DEFAULT '{}'::text[] NOT NULL,
    comment text DEFAULT ''::text NOT NULL,
    version bigint NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    search_text text DEFAULT ''::text NOT NULL,
    game_uuid uuid,
    move_uuid uuid,
    target_kind text GENERATED ALWAYS AS (
CASE
    WHEN (match_uuid IS NOT NULL) THEN 'match'::text
    WHEN (position_uuid IS NOT NULL) THEN 'position'::text
    WHEN (game_uuid IS NOT NULL) THEN 'game'::text
    ELSE 'move'::text
END) STORED,
    target_id uuid GENERATED ALWAYS AS (COALESCE(match_uuid, position_uuid, game_uuid, move_uuid)) STORED,
    CONSTRAINT annotations_comment_check CHECK ((char_length(comment) <= 20000)),
    CONSTRAINT annotations_one_target CHECK ((num_nonnulls(match_uuid, position_uuid, game_uuid, move_uuid) = 1)),
    CONSTRAINT annotations_tags_check CHECK ((cardinality(tags) <= 30)),
    CONSTRAINT annotations_version_check CHECK ((version > 0))
);


--
-- Name: app_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.app_settings (
    singleton boolean DEFAULT true NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    player_name text DEFAULT ''::text NOT NULL,
    analysis_engine text DEFAULT 'gnubg'::text NOT NULL,
    analysis_plies integer DEFAULT 3 NOT NULL,
    play_engine text DEFAULT 'gnubg'::text NOT NULL,
    play_plies integer DEFAULT 1 NOT NULL,
    gnubg_path text NOT NULL,
    gnubg_version text,
    bearoff_direction text DEFAULT 'right'::text NOT NULL,
    default_play_mode text DEFAULT 'coach'::text NOT NULL,
    tutor_threshold double precision DEFAULT 0.04 NOT NULL,
    training_auto_analysis boolean DEFAULT true NOT NULL,
    gnubg_threads integer,
    open_sage_threads integer,
    gnubg_play_threads integer,
    open_sage_play_threads integer,
    parallel_analysis_jobs integer,
    CONSTRAINT app_settings_analysis_engine_check CHECK ((analysis_engine = ANY (ARRAY['gnubg'::text, 'bgsage'::text]))),
    CONSTRAINT app_settings_analysis_plies_check CHECK (((analysis_plies >= 1) AND (analysis_plies <= 5))),
    CONSTRAINT app_settings_bearoff_direction_check CHECK ((bearoff_direction = ANY (ARRAY['right'::text, 'left'::text]))),
    CONSTRAINT app_settings_default_play_mode_check CHECK ((default_play_mode = ANY (ARRAY['competition'::text, 'training'::text, 'coach'::text, 'tutor'::text]))),
    CONSTRAINT app_settings_gnubg_play_threads_check CHECK ((gnubg_play_threads >= 1)),
    CONSTRAINT app_settings_gnubg_threads_check CHECK ((gnubg_threads >= 1)),
    CONSTRAINT app_settings_open_sage_play_threads_check CHECK ((open_sage_play_threads >= 2)),
    CONSTRAINT app_settings_open_sage_threads_check CHECK ((open_sage_threads >= 2)),
    CONSTRAINT app_settings_parallel_analysis_jobs_check CHECK ((parallel_analysis_jobs >= 1)),
    CONSTRAINT app_settings_play_engine_check CHECK ((play_engine = ANY (ARRAY['gnubg'::text, 'bgsage'::text]))),
    CONSTRAINT app_settings_play_plies_check CHECK (((play_plies >= 1) AND (play_plies <= 3))),
    CONSTRAINT app_settings_sage_analysis_depth_check CHECK (((analysis_engine <> 'bgsage'::text) OR ((analysis_plies >= 1) AND (analysis_plies <= 4)))),
    CONSTRAINT app_settings_sage_play_depth_check CHECK (((play_engine <> 'bgsage'::text) OR (play_plies = ANY (ARRAY[1, 2])))),
    CONSTRAINT app_settings_singleton_check CHECK (singleton),
    CONSTRAINT app_settings_tutor_threshold_check CHECK (((tutor_threshold > (0)::double precision) AND (tutor_threshold <= (1)::double precision))),
    CONSTRAINT app_settings_version_check CHECK ((version >= 0))
);


--
-- Name: benchmark_machine; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.benchmark_machine (
    singleton boolean DEFAULT true NOT NULL,
    id uuid NOT NULL,
    CONSTRAINT benchmark_machine_singleton_check CHECK (singleton)
);


--
-- Name: benchmark_runs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.benchmark_runs (
    id uuid NOT NULL,
    queue_no bigint NOT NULL,
    status text DEFAULT 'queued'::text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    machine jsonb NOT NULL,
    engine_id text NOT NULL,
    engine_name text NOT NULL,
    engine_version text NOT NULL,
    engine_path text NOT NULL,
    plies integer NOT NULL,
    reference jsonb,
    protocol jsonb,
    reference_sgf text,
    total_elapsed_ms double precision,
    average_move_ms double precision,
    completed_games integer DEFAULT 0 NOT NULL,
    analyzed_moves integer DEFAULT 0 NOT NULL,
    error text,
    worker_token uuid,
    lease_until timestamp with time zone,
    play_reference jsonb,
    completed_turns integer DEFAULT 0 NOT NULL,
    average_turn_ms double precision,
    engine_snapshot jsonb,
    startup_elapsed_ms double precision,
    CONSTRAINT benchmark_result CHECK ((((status = 'completed'::text) AND (finished_at IS NOT NULL) AND (total_elapsed_ms IS NOT NULL) AND (total_elapsed_ms > (0)::double precision) AND (total_elapsed_ms < 'Infinity'::double precision) AND (((play_reference IS NULL) AND (completed_games = 10) AND (analyzed_moves = 460) AND (average_move_ms IS NOT NULL) AND (average_move_ms > (0)::double precision) AND (average_move_ms < 'Infinity'::double precision)) OR ((play_reference IS NOT NULL) AND (completed_turns = ((play_reference ->> 'total_turns'::text))::integer) AND (average_turn_ms IS NOT NULL) AND (average_turn_ms > (0)::double precision) AND (average_turn_ms < 'Infinity'::double precision)))) OR ((status <> 'completed'::text) AND (total_elapsed_ms IS NULL) AND (average_move_ms IS NULL) AND (average_turn_ms IS NULL)))),
    CONSTRAINT benchmark_runs_analyzed_moves_check CHECK (((analyzed_moves >= 0) AND (analyzed_moves <= 460))),
    CONSTRAINT benchmark_runs_completed_games_check CHECK (((completed_games >= 0) AND (completed_games <= 10))),
    CONSTRAINT benchmark_runs_plies_check CHECK (((plies >= 1) AND (plies <= 5))),
    CONSTRAINT benchmark_runs_startup_elapsed_ms_check CHECK ((startup_elapsed_ms >= (0)::double precision)),
    CONSTRAINT benchmark_runs_status_check CHECK ((status = ANY (ARRAY['queued'::text, 'running'::text, 'completed'::text, 'failed'::text, 'cancelled'::text]))),
    CONSTRAINT benchmark_workload CHECK ((((play_reference IS NULL) AND (reference IS NOT NULL) AND (protocol IS NOT NULL) AND (reference_sgf IS NOT NULL) AND (completed_turns = 0) AND (average_turn_ms IS NULL)) OR ((play_reference IS NOT NULL) AND (reference IS NULL) AND (protocol IS NULL) AND (reference_sgf IS NULL) AND ((plies >= 1) AND (plies <= 3)) AND (completed_games = 0) AND (analyzed_moves = 0) AND (average_move_ms IS NULL) AND (((play_reference ->> 'total_turns'::text))::integer IS NOT NULL) AND ((completed_turns >= 0) AND (completed_turns <= ((play_reference ->> 'total_turns'::text))::integer)))))
);


--
-- Name: benchmark_runs_queue_no_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.benchmark_runs_queue_no_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: benchmark_runs_queue_no_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.benchmark_runs_queue_no_seq OWNED BY public.benchmark_runs.queue_no;


--
-- Name: card_practice_items; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.card_practice_items (
    round_id uuid NOT NULL,
    pass smallint NOT NULL,
    ordinal integer NOT NULL,
    card_id uuid NOT NULL,
    answered_by uuid,
    submitted jsonb,
    result jsonb,
    marked boolean DEFAULT false NOT NULL,
    advanced boolean DEFAULT false NOT NULL,
    question jsonb,
    diagnosis text,
    lesson text,
    CONSTRAINT card_practice_items_check CHECK (((answered_by IS NULL) = (result IS NULL))),
    CONSTRAINT card_practice_items_check1 CHECK (((NOT marked) OR (result IS NOT NULL))),
    CONSTRAINT card_practice_items_ordinal_check CHECK ((ordinal >= 0)),
    CONSTRAINT card_practice_items_pass_check CHECK ((pass = ANY (ARRAY[1, 2])))
);


--
-- Name: card_practice_rounds; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.card_practice_rounds (
    id uuid NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    pass smallint DEFAULT 1 NOT NULL,
    transfer jsonb,
    completed_at timestamp with time zone,
    started_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT card_practice_rounds_pass_check CHECK ((pass = ANY (ARRAY[1, 2]))),
    CONSTRAINT card_practice_rounds_version_check CHECK ((version >= 0))
);


--
-- Name: card_reviews; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.card_reviews (
    card_id uuid NOT NULL,
    request_id uuid NOT NULL,
    session_id uuid NOT NULL,
    rating text NOT NULL,
    result jsonb NOT NULL,
    reviewed_at timestamp with time zone NOT NULL,
    CONSTRAINT card_reviews_rating_check CHECK ((rating = ANY (ARRAY['again'::text, 'hard'::text, 'good'::text, 'easy'::text])))
);


--
-- Name: card_topics; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.card_topics (
    id uuid NOT NULL,
    name text NOT NULL,
    filter jsonb NOT NULL,
    CONSTRAINT card_topics_name_check CHECK (((length(name) >= 1) AND (length(name) <= 120)))
);


--
-- Name: configuration_profiles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.configuration_profiles (
    id uuid NOT NULL,
    name text NOT NULL,
    name_key text NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    configuration jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT configuration_profiles_configuration_check CHECK ((jsonb_typeof(configuration) = 'object'::text)),
    CONSTRAINT configuration_profiles_name_check CHECK (((char_length(name) >= 1) AND (char_length(name) <= 80))),
    CONSTRAINT configuration_profiles_revision_check CHECK ((revision > 0))
);


--
-- Name: configuration_state; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.configuration_state (
    singleton boolean DEFAULT true NOT NULL,
    revision bigint DEFAULT 1 NOT NULL,
    active_profile_id uuid NOT NULL,
    current_configuration jsonb NOT NULL,
    legacy_preferences_imported boolean DEFAULT true NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT configuration_state_current_configuration_check CHECK ((jsonb_typeof(current_configuration) = 'object'::text)),
    CONSTRAINT configuration_state_revision_check CHECK ((revision > 0)),
    CONSTRAINT configuration_state_singleton_check CHECK (singleton)
);


--
-- Name: databases; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.databases (
    id uuid NOT NULL,
    name text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    version bigint DEFAULT 1 NOT NULL,
    is_default boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    collection_only boolean DEFAULT false NOT NULL,
    is_system boolean GENERATED ALWAYS AS ((is_default OR (id = ANY (ARRAY['00000000-0000-0000-0000-000000000003'::uuid, '00000000-0000-0000-0000-000000000004'::uuid, '00000000-0000-0000-0000-000000000005'::uuid])))) STORED,
    CONSTRAINT databases_collection_identity CHECK (((collection_only = (id = '00000000-0000-0000-0000-000000000002'::uuid)) AND (NOT (collection_only AND is_default)))),
    CONSTRAINT databases_description_check CHECK ((char_length(description) <= 2000)),
    CONSTRAINT databases_engine_duels_name CHECK (((id <> '00000000-0000-0000-0000-000000000005'::uuid) OR (name = 'Engine Duels'::text))),
    CONSTRAINT databases_name_check CHECK (((char_length(btrim(name)) >= 1) AND (char_length(btrim(name)) <= 120))),
    CONSTRAINT databases_paused_matches_name CHECK (((id <> '00000000-0000-0000-0000-000000000004'::uuid) OR (name = 'Paused Matches'::text))),
    CONSTRAINT databases_system_names CHECK ((((NOT is_default) OR (name = 'Unsorted'::text)) AND ((id <> '00000000-0000-0000-0000-000000000003'::uuid) OR (name = 'Bot Matches'::text)))),
    CONSTRAINT databases_version_check CHECK ((version > 0))
);


--
-- Name: decision_index; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.decision_index (
    match_id uuid NOT NULL,
    seq integer NOT NULL,
    game_no integer NOT NULL,
    decision_no integer NOT NULL,
    player_name text NOT NULL,
    question_kind text NOT NULL,
    game_plan text NOT NULL,
    opportunity text,
    mistake_kind text NOT NULL,
    equity_loss double precision NOT NULL,
    match_length integer NOT NULL,
    away_own integer,
    away_opponent integer,
    cube_value integer NOT NULL,
    cube_owner text NOT NULL,
    played_on date NOT NULL,
    hit jsonb NOT NULL,
    CONSTRAINT decision_index_cube_owner_check CHECK ((cube_owner = ANY (ARRAY['self'::text, 'opponent'::text, 'centered'::text]))),
    CONSTRAINT decision_index_question_kind_check CHECK ((question_kind = ANY (ARRAY['checker'::text, 'cube_offer'::text, 'cube_response'::text])))
);


--
-- Name: decision_index_matches; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.decision_index_matches (
    match_id uuid NOT NULL,
    fingerprint text NOT NULL,
    indexed_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: deck_cards; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.deck_cards (
    deck_id uuid NOT NULL,
    card_id uuid NOT NULL
);


--
-- Name: decks; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.decks (
    id uuid NOT NULL,
    name text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    CONSTRAINT decks_name_check CHECK (((length(name) >= 1) AND (length(name) <= 120))),
    CONSTRAINT decks_version_check CHECK ((version >= 0))
);


--
-- Name: effective_analysis_selections; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.effective_analysis_selections AS
 SELECT m.id AS match_id,
    chosen.analysis_id,
    chosen.job_id,
    chosen.source
   FROM ((public.matches m
     LEFT JOIN public.analysis_selections selected ON ((selected.match_id = m.id)))
     JOIN LATERAL ( SELECT a.analysis_id,
            a.job_id,
            a.source
           FROM public.analysis_catalog_sources a
          WHERE ((a.match_id = m.id) AND
                CASE
                    WHEN (selected.analysis_id IS NOT NULL) THEN ((a.analysis_id = selected.analysis_id) AND (selected.source_revision = m.source_revision))
                    ELSE ((a.status = 'completed'::text) AND ((a.source = 'imported'::text) OR (a.scope <> 'match'::text) OR a.complete OR m.collection_only))
                END)
          ORDER BY
                CASE
                    WHEN ((a.source = 'native'::text) AND (a.scope = 'match'::text) AND a.complete) THEN 0
                    WHEN (a.source = 'imported'::text) THEN 1
                    ELSE 2
                END, a.created_at DESC, a.analysis_id DESC
         LIMIT 1) chosen ON (true));


--
-- Name: engine_installations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.engine_installations (
    engine_id text NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    python_path text DEFAULT ''::text NOT NULL,
    snapshot jsonb,
    CONSTRAINT engine_installations_engine_id_check CHECK ((engine_id = 'bgsage'::text)),
    CONSTRAINT engine_installations_version_check CHECK ((version >= 0))
);


--
-- Name: engine_match_results; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.engine_match_results (
    run_id uuid NOT NULL,
    match_index integer NOT NULL,
    seed uuid NOT NULL,
    score_a integer NOT NULL,
    score_b integer NOT NULL,
    games integer NOT NULL,
    sgf text NOT NULL,
    cpu_us_a bigint,
    wall_us_a bigint,
    moves_a integer,
    cpu_us_b bigint,
    wall_us_b bigint,
    moves_b integer,
    outcomes integer[],
    CONSTRAINT engine_match_results_check CHECK (((score_b >= 0) AND ((outcomes IS NOT NULL) OR (score_a <> score_b)))),
    CONSTRAINT engine_match_results_cost_a CHECK ((((cpu_us_a IS NULL) AND (wall_us_a IS NULL) AND (moves_a IS NULL)) OR ((cpu_us_a >= 0) AND (wall_us_a >= 0) AND (moves_a >= 0)))),
    CONSTRAINT engine_match_results_cost_b CHECK ((((cpu_us_b IS NULL) AND (wall_us_b IS NULL) AND (moves_b IS NULL)) OR ((cpu_us_b >= 0) AND (wall_us_b >= 0) AND (moves_b >= 0)))),
    CONSTRAINT engine_match_results_games_check CHECK ((games > 0)),
    CONSTRAINT engine_match_results_match_index_check CHECK ((match_index >= 0)),
    CONSTRAINT engine_match_results_outcomes CHECK (((outcomes IS NULL) OR ((cardinality(outcomes) = 6) AND (array_ndims(outcomes) = 1) AND (0 <= ALL (outcomes)) AND (score_a = ((outcomes[1] + (2 * outcomes[2])) + (3 * outcomes[3]))) AND (score_b = ((outcomes[4] + (2 * outcomes[5])) + (3 * outcomes[6]))) AND (games = (((((outcomes[1] + outcomes[2]) + outcomes[3]) + outcomes[4]) + outcomes[5]) + outcomes[6]))))),
    CONSTRAINT engine_match_results_score_a_check CHECK ((score_a >= 0)),
    CONSTRAINT engine_match_results_sgf_check CHECK ((sgf <> ''::text))
);


--
-- Name: engine_match_runs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.engine_match_runs (
    id uuid NOT NULL,
    configuration jsonb NOT NULL,
    engines jsonb NOT NULL,
    dice_version text NOT NULL,
    status text DEFAULT 'queued'::text NOT NULL,
    total_matches integer NOT NULL,
    completed_matches integer DEFAULT 0 NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    lease_token uuid,
    lease_until timestamp with time zone,
    error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT engine_match_runs_check CHECK (((completed_matches >= 0) AND (completed_matches <= total_matches))),
    CONSTRAINT engine_match_runs_status_check CHECK ((status = ANY (ARRAY['queued'::text, 'running'::text, 'completed'::text, 'failed'::text, 'cancelled'::text]))),
    CONSTRAINT engine_match_runs_total_matches_check CHECK (((total_matches >= 1) AND (total_matches <= 2000)))
);


--
-- Name: imported_analysis_projections; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.imported_analysis_projections (
    imported_analysis_id uuid NOT NULL,
    revision integer NOT NULL,
    source_revision bigint DEFAULT 0 NOT NULL,
    projection jsonb,
    CONSTRAINT imported_analysis_projections_source_revision_check CHECK ((source_revision >= 0))
);


--
-- Name: job_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.job_events (
    id bigint NOT NULL,
    job_id uuid NOT NULL,
    event text NOT NULL,
    data jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: job_events_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.job_events_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: job_events_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.job_events_id_seq OWNED BY public.job_events.id;


--
-- Name: learning_card_exports; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.learning_card_exports (
    card_id uuid NOT NULL,
    fingerprint text NOT NULL,
    modified bigint NOT NULL,
    CONSTRAINT learning_card_exports_fingerprint_check CHECK ((length(fingerprint) = 64)),
    CONSTRAINT learning_card_exports_modified_check CHECK ((modified > 0))
);


--
-- Name: learning_card_game_plans; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.learning_card_game_plans (
    card_id uuid NOT NULL,
    classifier text NOT NULL,
    game_plan text
);


--
-- Name: learning_cards; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.learning_cards (
    id uuid NOT NULL,
    signature text NOT NULL,
    payload jsonb NOT NULL,
    stability double precision,
    difficulty double precision,
    last_review timestamp with time zone,
    due_at timestamp with time zone DEFAULT now() NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    review_count integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    tags text[] DEFAULT '{}'::text[] NOT NULL,
    tags_version bigint DEFAULT 0 NOT NULL,
    anki_review_count integer DEFAULT 0 NOT NULL,
    CONSTRAINT learning_cards_anki_review_count_check CHECK ((anki_review_count >= 0)),
    CONSTRAINT learning_cards_check CHECK ((((stability IS NULL) AND (difficulty IS NULL) AND (last_review IS NULL)) OR ((stability IS NOT NULL) AND (difficulty IS NOT NULL) AND (last_review IS NOT NULL)))),
    CONSTRAINT learning_cards_difficulty_check CHECK (((difficulty >= (1)::double precision) AND (difficulty <= (10)::double precision))),
    CONSTRAINT learning_cards_review_count_check CHECK ((review_count >= 0)),
    CONSTRAINT learning_cards_stability_check CHECK (((stability > (0)::double precision) AND (stability < 'Infinity'::double precision))),
    CONSTRAINT learning_cards_tags_version_check CHECK ((tags_version >= 0)),
    CONSTRAINT learning_cards_version_check CHECK ((version >= 0))
);


--
-- Name: learning_preferences; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.learning_preferences (
    id boolean DEFAULT true NOT NULL,
    new_cards_per_day integer DEFAULT 10 NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT learning_preferences_id_check CHECK (id),
    CONSTRAINT learning_preferences_new_cards_per_day_check CHECK (((new_cards_per_day >= 0) AND (new_cards_per_day <= 1000)))
);


--
-- Name: match_analysis_timing; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.match_analysis_timing (
    job_id uuid NOT NULL,
    total_moves bigint NOT NULL,
    completed_moves bigint NOT NULL,
    timed_moves bigint DEFAULT 0 NOT NULL,
    measured_elapsed_ms double precision DEFAULT 0 NOT NULL,
    benchmark_id uuid,
    benchmark_average_move_ms double precision,
    lease_token uuid NOT NULL,
    attempt integer NOT NULL,
    active_elapsed_ms double precision DEFAULT 0 NOT NULL,
    active_started_at timestamp with time zone,
    CONSTRAINT match_analysis_timing_active_elapsed_ms_check CHECK (((active_elapsed_ms >= (0)::double precision) AND (active_elapsed_ms < 'Infinity'::double precision))),
    CONSTRAINT match_analysis_timing_benchmark_average_move_ms_check CHECK (((benchmark_average_move_ms > (0)::double precision) AND (benchmark_average_move_ms < 'Infinity'::double precision))),
    CONSTRAINT match_analysis_timing_check CHECK (((completed_moves >= 0) AND (completed_moves <= total_moves))),
    CONSTRAINT match_analysis_timing_check1 CHECK (((timed_moves >= 0) AND (timed_moves <= completed_moves))),
    CONSTRAINT match_analysis_timing_measured_elapsed_ms_check CHECK (((measured_elapsed_ms >= (0)::double precision) AND (measured_elapsed_ms < 'Infinity'::double precision))),
    CONSTRAINT match_analysis_timing_total_moves_check CHECK ((total_moves >= 0))
);


--
-- Name: match_assessments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.match_assessments (
    id uuid NOT NULL,
    report_id uuid NOT NULL,
    report_version bigint NOT NULL,
    language text NOT NULL,
    prompt_version integer NOT NULL,
    model text NOT NULL,
    status text NOT NULL,
    result jsonb,
    error_code text,
    lease_token uuid,
    lease_until timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT match_assessments_check CHECK (((status = 'completed'::text) = (result IS NOT NULL))),
    CONSTRAINT match_assessments_check1 CHECK (((status = 'failed'::text) = (error_code IS NOT NULL))),
    CONSTRAINT match_assessments_check2 CHECK (((status = 'running'::text) = ((lease_token IS NOT NULL) AND (lease_until IS NOT NULL)))),
    CONSTRAINT match_assessments_language_check CHECK ((language = ANY (ARRAY['de'::text, 'en'::text, 'es'::text, 'fr'::text, 'it'::text, 'pl'::text]))),
    CONSTRAINT match_assessments_status_check CHECK ((status = ANY (ARRAY['queued'::text, 'running'::text, 'completed'::text, 'failed'::text])))
);


--
-- Name: match_games; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.match_games (
    match_uuid uuid NOT NULL,
    game_no integer NOT NULL,
    position_count integer NOT NULL,
    white_score integer NOT NULL,
    black_score integer NOT NULL,
    crawford boolean NOT NULL,
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    CONSTRAINT match_games_black_score_check CHECK ((black_score >= 0)),
    CONSTRAINT match_games_game_no_check CHECK ((game_no > 0)),
    CONSTRAINT match_games_position_count_check CHECK ((position_count >= 0)),
    CONSTRAINT match_games_white_score_check CHECK ((white_score >= 0))
);


--
-- Name: match_moves; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.match_moves (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    match_uuid uuid NOT NULL,
    game_no integer NOT NULL,
    move_no integer NOT NULL,
    kind text NOT NULL,
    player integer NOT NULL,
    notation text NOT NULL,
    position_uuid uuid,
    board integer[] NOT NULL,
    dice integer[] NOT NULL,
    position_id text NOT NULL,
    gnubg_match_id text NOT NULL,
    CONSTRAINT match_moves_board_check CHECK ((array_length(board, 1) = 26)),
    CONSTRAINT match_moves_dice_check CHECK ((array_length(dice, 1) = 2)),
    CONSTRAINT match_moves_kind_check CHECK ((kind = ANY (ARRAY['checker'::text, 'cube'::text, 'resign'::text]))),
    CONSTRAINT match_moves_move_no_check CHECK ((move_no > 0)),
    CONSTRAINT match_moves_player_check CHECK ((player = ANY (ARRAY[0, 1])))
);


--
-- Name: match_players; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.match_players (
    match_id uuid NOT NULL,
    side smallint NOT NULL,
    alias_id uuid NOT NULL,
    CONSTRAINT match_players_side_check CHECK ((side = ANY (ARRAY[0, 1])))
);


--
-- Name: matches_match_no_seq; Type: SEQUENCE; Schema: public; Owner: -
--

ALTER TABLE public.matches ALTER COLUMN match_no ADD GENERATED ALWAYS AS IDENTITY (
    SEQUENCE NAME public.matches_match_no_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1
);


--
-- Name: mistake_practice_attempts; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.mistake_practice_attempts (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    attempted_at timestamp with time zone DEFAULT now() NOT NULL,
    player text NOT NULL,
    game_plan text NOT NULL,
    mistake_kind text NOT NULL,
    verdict text NOT NULL,
    equity_loss double precision NOT NULL,
    CONSTRAINT mistake_practice_attempts_equity_loss_check CHECK ((equity_loss >= (0)::double precision)),
    CONSTRAINT mistake_practice_attempts_verdict_check CHECK ((verdict = ANY (ARRAY['best'::text, 'playable'::text, 'error'::text, 'forced'::text])))
);


--
-- Name: move_analyses_id_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.move_analyses_id_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: move_analyses_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.move_analyses_id_seq OWNED BY public.move_analyses.id;


--
-- Name: opening_book_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.opening_book_settings (
    singleton boolean DEFAULT true NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    provider text DEFAULT 'automatic'::text NOT NULL,
    path text NOT NULL,
    xg_path text DEFAULT ''::text NOT NULL,
    CONSTRAINT opening_book_settings_provider_check CHECK ((provider = ANY (ARRAY['off'::text, 'backgammondb'::text, 'automatic'::text, 'xg'::text]))),
    CONSTRAINT opening_book_settings_singleton_check CHECK (singleton),
    CONSTRAINT opening_book_settings_version_check CHECK ((version >= 0))
);


--
-- Name: player_aliases; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.player_aliases (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    platform text NOT NULL,
    name text NOT NULL,
    profile_id uuid NOT NULL,
    CONSTRAINT player_aliases_name_check CHECK ((name <> ''::text)),
    CONSTRAINT player_aliases_platform_check CHECK ((platform = ANY (ARRAY[''::text, 'backgammongalaxy.com'::text, 'opengammon.com'::text])))
);


--
-- Name: player_profiles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.player_profiles (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    source_name text NOT NULL,
    first_name text DEFAULT ''::text NOT NULL,
    last_name text DEFAULT ''::text NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    CONSTRAINT player_profiles_first_name_check CHECK ((char_length(first_name) <= 120)),
    CONSTRAINT player_profiles_last_name_check CHECK ((char_length(last_name) <= 120))
);


--
-- Name: position_filters; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.position_filters (
    id uuid NOT NULL,
    name text NOT NULL,
    definition jsonb NOT NULL,
    version bigint DEFAULT 1 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT position_filters_definition_check CHECK (((jsonb_typeof(definition) = 'object'::text) AND ((definition -> 'schema_version'::text) = '1'::jsonb))),
    CONSTRAINT position_filters_name_check CHECK (((name = btrim(name)) AND ((char_length(name) >= 1) AND (char_length(name) <= 120)))),
    CONSTRAINT position_filters_version_check CHECK ((version > 0))
);


--
-- Name: position_game_plans; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.position_game_plans (
    position_uuid uuid NOT NULL,
    classifier text NOT NULL,
    game_plan text,
    CONSTRAINT position_game_plans_game_plan_check CHECK ((game_plan = ANY (ARRAY['opening'::text, 'pure_race'::text, 'racing'::text, 'attacking'::text, 'priming'::text, 'anchoring'::text])))
);


--
-- Name: review_hit_sets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.review_hit_sets (
    match_id uuid NOT NULL,
    analysis_id text NOT NULL,
    fingerprint text NOT NULL,
    indexed_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: review_hits; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.review_hits (
    match_id uuid NOT NULL,
    analysis_id text NOT NULL,
    game_no integer NOT NULL,
    decision_no integer NOT NULL,
    checker boolean NOT NULL,
    id text NOT NULL,
    player integer NOT NULL,
    evaluated boolean NOT NULL,
    equity_loss double precision NOT NULL,
    CONSTRAINT review_hits_player_check CHECK ((player = ANY (ARRAY[0, 1])))
);


--
-- Name: review_sessions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.review_sessions (
    id uuid NOT NULL,
    card_id uuid NOT NULL,
    card_version bigint NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    attempt_request_id uuid,
    attempt_input jsonb,
    attempt_result jsonb,
    CONSTRAINT review_sessions_complete_attempt CHECK ((num_nonnulls(attempt_request_id, attempt_input, attempt_result) = ANY (ARRAY[0, 3])))
);


--
-- Name: saved_positions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.saved_positions (
    position_uuid uuid NOT NULL,
    saved_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: score_practice_profiles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.score_practice_profiles (
    id uuid NOT NULL,
    card_id uuid NOT NULL,
    card_version bigint NOT NULL,
    plies integer NOT NULL,
    engine jsonb NOT NULL,
    expires_at timestamp with time zone DEFAULT (now() + '1 day'::interval) NOT NULL
);


--
-- Name: similar_practice_summaries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.similar_practice_summaries (
    id uuid NOT NULL,
    completed_at timestamp with time zone NOT NULL,
    total_positions integer NOT NULL,
    scored_moves integer NOT NULL,
    missed_moves integer NOT NULL,
    total_equity_loss double precision NOT NULL,
    is_retry boolean NOT NULL,
    CONSTRAINT similar_practice_summaries_check CHECK (((scored_moves >= 1) AND (scored_moves <= total_positions))),
    CONSTRAINT similar_practice_summaries_check1 CHECK (((missed_moves >= 0) AND (missed_moves <= scored_moves))),
    CONSTRAINT similar_practice_summaries_total_equity_loss_check CHECK (((total_equity_loss >= (0)::double precision) AND (total_equity_loss < 'Infinity'::double precision))),
    CONSTRAINT similar_practice_summaries_total_positions_check CHECK (((total_positions >= 1) AND (total_positions <= 50)))
);


--
-- Name: study_report_snapshots; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.study_report_snapshots (
    id uuid NOT NULL,
    session_id uuid NOT NULL,
    game_no integer NOT NULL,
    library_match_id uuid,
    state jsonb NOT NULL,
    sgf text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT study_report_snapshots_game_no_check CHECK ((game_no >= 0)),
    CONSTRAINT study_report_snapshots_state_check CHECK ((jsonb_typeof((state -> 'review'::text)) = 'object'::text))
);


--
-- Name: transfer_practice_seen; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.transfer_practice_seen (
    signature text NOT NULL
);


--
-- Name: virtual_player_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.virtual_player_members (
    profile_id uuid NOT NULL,
    virtual_id uuid NOT NULL
);


--
-- Name: virtual_player_profiles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.virtual_player_profiles (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    name text NOT NULL,
    is_me boolean DEFAULT false NOT NULL,
    version bigint DEFAULT 0 NOT NULL,
    CONSTRAINT virtual_player_profiles_name_check CHECK (((btrim(name) = name) AND (name <> ''::text) AND (char_length(name) <= 120)))
);


--
-- Name: virtual_player_sides; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.virtual_player_sides AS
 SELECT v.virtual_id,
    mp.match_id,
    mp.side,
    true AS rated
   FROM ((public.virtual_player_members v
     JOIN public.player_aliases a ON ((a.profile_id = v.profile_id)))
     JOIN public.match_players mp ON ((mp.alias_id = a.id)))
UNION ALL
 SELECT vp.id AS virtual_id,
    m.id AS match_id,
    ((s.state ->> 'human_player'::text))::smallint AS side,
    ((NOT m.play_assisted) AND (m.play_mode = 'competition'::text)) AS rated
   FROM ((public.virtual_player_profiles vp
     JOIN public.play_sessions s ON (vp.is_me))
     JOIN public.matches m ON ((m.id = s.library_match_id)))
  WHERE ((m.play_mode IS NOT NULL) AND (NOT m.collection_only) AND ((s.state ->> 'human_player'::text) = ANY (ARRAY['0'::text, '1'::text])) AND (NOT ((COALESCE((s.state ->> 'play_mode'::text), 'competition'::text) = 'competition'::text) AND ((s.state ->> 'phase'::text) <> 'match_over'::text) AND (NOT (((s.state ->> 'match_length'::text) = '0'::text) AND ((s.state ->> 'phase'::text) = 'game_over'::text))))));


--
-- Name: benchmark_runs queue_no; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.benchmark_runs ALTER COLUMN queue_no SET DEFAULT nextval('public.benchmark_runs_queue_no_seq'::regclass);


--
-- Name: job_events id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_events ALTER COLUMN id SET DEFAULT nextval('public.job_events_id_seq'::regclass);


--
-- Name: move_analyses id; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.move_analyses ALTER COLUMN id SET DEFAULT nextval('public.move_analyses_id_seq'::regclass);


--
-- Name: analysis_selections analysis_selections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.analysis_selections
    ADD CONSTRAINT analysis_selections_pkey PRIMARY KEY (match_id);


--
-- Name: anki_auto_sync anki_auto_sync_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_auto_sync
    ADD CONSTRAINT anki_auto_sync_pkey PRIMARY KEY (deck_id);


--
-- Name: anki_card_faces anki_card_faces_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_card_faces
    ADD CONSTRAINT anki_card_faces_pkey PRIMARY KEY (card_id);


--
-- Name: anki_media anki_media_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_media
    ADD CONSTRAINT anki_media_pkey PRIMARY KEY (package, name);


--
-- Name: anki_received_reviews anki_received_reviews_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_received_reviews
    ADD CONSTRAINT anki_received_reviews_pkey PRIMARY KEY (card_id, review_id);


--
-- Name: anki_review_exports anki_review_exports_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_review_exports
    ADD CONSTRAINT anki_review_exports_pkey PRIMARY KEY (card_id, request_id);


--
-- Name: anki_review_exports anki_review_exports_review_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_review_exports
    ADD CONSTRAINT anki_review_exports_review_id_key UNIQUE (review_id);


--
-- Name: anki_source_decks anki_source_decks_deck_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_source_decks
    ADD CONSTRAINT anki_source_decks_deck_id_key UNIQUE (deck_id);


--
-- Name: anki_source_decks anki_source_decks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_source_decks
    ADD CONSTRAINT anki_source_decks_pkey PRIMARY KEY (source_name);


--
-- Name: anki_sync_history anki_sync_history_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_sync_history
    ADD CONSTRAINT anki_sync_history_pkey PRIMARY KEY (id);


--
-- Name: annotations annotations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.annotations
    ADD CONSTRAINT annotations_pkey PRIMARY KEY (id);


--
-- Name: annotations annotations_target_kind_target_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.annotations
    ADD CONSTRAINT annotations_target_kind_target_id_key UNIQUE (target_kind, target_id);


--
-- Name: app_settings app_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.app_settings
    ADD CONSTRAINT app_settings_pkey PRIMARY KEY (singleton);


--
-- Name: benchmark_machine benchmark_machine_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.benchmark_machine
    ADD CONSTRAINT benchmark_machine_id_key UNIQUE (id);


--
-- Name: benchmark_machine benchmark_machine_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.benchmark_machine
    ADD CONSTRAINT benchmark_machine_pkey PRIMARY KEY (singleton);


--
-- Name: benchmark_runs benchmark_runs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.benchmark_runs
    ADD CONSTRAINT benchmark_runs_pkey PRIMARY KEY (id);


--
-- Name: benchmark_runs benchmark_runs_queue_no_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.benchmark_runs
    ADD CONSTRAINT benchmark_runs_queue_no_key UNIQUE (queue_no);


--
-- Name: card_practice_items card_practice_items_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.card_practice_items
    ADD CONSTRAINT card_practice_items_pkey PRIMARY KEY (round_id, pass, ordinal);


--
-- Name: card_practice_items card_practice_items_round_id_pass_card_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.card_practice_items
    ADD CONSTRAINT card_practice_items_round_id_pass_card_id_key UNIQUE (round_id, pass, card_id);


--
-- Name: card_practice_rounds card_practice_rounds_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.card_practice_rounds
    ADD CONSTRAINT card_practice_rounds_pkey PRIMARY KEY (id);


--
-- Name: card_reviews card_reviews_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.card_reviews
    ADD CONSTRAINT card_reviews_pkey PRIMARY KEY (card_id, request_id);


--
-- Name: card_reviews card_reviews_session_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.card_reviews
    ADD CONSTRAINT card_reviews_session_id_key UNIQUE (session_id);


--
-- Name: card_topics card_topics_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.card_topics
    ADD CONSTRAINT card_topics_pkey PRIMARY KEY (id);


--
-- Name: configuration_profiles configuration_profiles_name_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.configuration_profiles
    ADD CONSTRAINT configuration_profiles_name_key_key UNIQUE (name_key);


--
-- Name: configuration_profiles configuration_profiles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.configuration_profiles
    ADD CONSTRAINT configuration_profiles_pkey PRIMARY KEY (id);


--
-- Name: configuration_state configuration_state_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.configuration_state
    ADD CONSTRAINT configuration_state_pkey PRIMARY KEY (singleton);


--
-- Name: databases databases_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.databases
    ADD CONSTRAINT databases_pkey PRIMARY KEY (id);


--
-- Name: decision_index_matches decision_index_matches_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.decision_index_matches
    ADD CONSTRAINT decision_index_matches_pkey PRIMARY KEY (match_id);


--
-- Name: decision_index decision_index_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.decision_index
    ADD CONSTRAINT decision_index_pkey PRIMARY KEY (match_id, seq);


--
-- Name: deck_cards deck_cards_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.deck_cards
    ADD CONSTRAINT deck_cards_pkey PRIMARY KEY (deck_id, card_id);


--
-- Name: decks decks_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.decks
    ADD CONSTRAINT decks_pkey PRIMARY KEY (id);


--
-- Name: engine_installations engine_installations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.engine_installations
    ADD CONSTRAINT engine_installations_pkey PRIMARY KEY (engine_id);


--
-- Name: engine_match_results engine_match_results_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.engine_match_results
    ADD CONSTRAINT engine_match_results_pkey PRIMARY KEY (run_id, match_index);


--
-- Name: engine_match_runs engine_match_runs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.engine_match_runs
    ADD CONSTRAINT engine_match_runs_pkey PRIMARY KEY (id);


--
-- Name: imported_analyses imported_analyses_match_uuid_source_ordinal_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.imported_analyses
    ADD CONSTRAINT imported_analyses_match_uuid_source_ordinal_key UNIQUE (match_uuid, source_ordinal);


--
-- Name: imported_analyses imported_analyses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.imported_analyses
    ADD CONSTRAINT imported_analyses_pkey PRIMARY KEY (id);


--
-- Name: imported_analysis_projections imported_analysis_projections_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.imported_analysis_projections
    ADD CONSTRAINT imported_analysis_projections_pkey PRIMARY KEY (imported_analysis_id);


--
-- Name: job_events job_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_events
    ADD CONSTRAINT job_events_pkey PRIMARY KEY (id);


--
-- Name: jobs jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_pkey PRIMARY KEY (id);


--
-- Name: learning_card_exports learning_card_exports_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.learning_card_exports
    ADD CONSTRAINT learning_card_exports_pkey PRIMARY KEY (card_id);


--
-- Name: learning_card_game_plans learning_card_game_plans_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.learning_card_game_plans
    ADD CONSTRAINT learning_card_game_plans_pkey PRIMARY KEY (card_id, classifier);


--
-- Name: learning_cards learning_cards_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.learning_cards
    ADD CONSTRAINT learning_cards_pkey PRIMARY KEY (id);


--
-- Name: learning_cards learning_cards_signature_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.learning_cards
    ADD CONSTRAINT learning_cards_signature_key UNIQUE (signature);


--
-- Name: learning_preferences learning_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.learning_preferences
    ADD CONSTRAINT learning_preferences_pkey PRIMARY KEY (id);


--
-- Name: match_analysis_games match_analysis_games_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_analysis_games
    ADD CONSTRAINT match_analysis_games_pkey PRIMARY KEY (job_id, game_no);


--
-- Name: match_analysis_timing match_analysis_timing_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_analysis_timing
    ADD CONSTRAINT match_analysis_timing_pkey PRIMARY KEY (job_id);


--
-- Name: match_assessments match_assessments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_assessments
    ADD CONSTRAINT match_assessments_pkey PRIMARY KEY (id);


--
-- Name: match_assessments match_assessments_report_id_report_version_language_prompt__key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_assessments
    ADD CONSTRAINT match_assessments_report_id_report_version_language_prompt__key UNIQUE (report_id, report_version, language, prompt_version);


--
-- Name: match_games match_games_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_games
    ADD CONSTRAINT match_games_id_key UNIQUE (id);


--
-- Name: match_games match_games_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_games
    ADD CONSTRAINT match_games_pkey PRIMARY KEY (match_uuid, game_no);


--
-- Name: match_moves match_moves_match_uuid_game_no_move_no_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_moves
    ADD CONSTRAINT match_moves_match_uuid_game_no_move_no_key UNIQUE (match_uuid, game_no, move_no);


--
-- Name: match_moves match_moves_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_moves
    ADD CONSTRAINT match_moves_pkey PRIMARY KEY (id);


--
-- Name: match_players match_players_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_players
    ADD CONSTRAINT match_players_pkey PRIMARY KEY (match_id, side);


--
-- Name: matches matches_database_sha256_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.matches
    ADD CONSTRAINT matches_database_sha256_key UNIQUE (database_id, sha256);


--
-- Name: matches matches_match_no_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.matches
    ADD CONSTRAINT matches_match_no_key UNIQUE (match_no);


--
-- Name: matches matches_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.matches
    ADD CONSTRAINT matches_pkey PRIMARY KEY (id);


--
-- Name: mistake_practice_attempts mistake_practice_attempts_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.mistake_practice_attempts
    ADD CONSTRAINT mistake_practice_attempts_pkey PRIMARY KEY (id);


--
-- Name: move_analyses move_analyses_job_id_game_no_decision_no_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.move_analyses
    ADD CONSTRAINT move_analyses_job_id_game_no_decision_no_key UNIQUE (job_id, game_no, decision_no);


--
-- Name: move_analyses move_analyses_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.move_analyses
    ADD CONSTRAINT move_analyses_pkey PRIMARY KEY (id);


--
-- Name: opening_book_settings opening_book_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.opening_book_settings
    ADD CONSTRAINT opening_book_settings_pkey PRIMARY KEY (singleton);


--
-- Name: play_sessions play_sessions_library_match_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.play_sessions
    ADD CONSTRAINT play_sessions_library_match_id_key UNIQUE (library_match_id);


--
-- Name: play_sessions play_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.play_sessions
    ADD CONSTRAINT play_sessions_pkey PRIMARY KEY (id);


--
-- Name: player_aliases player_aliases_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_aliases
    ADD CONSTRAINT player_aliases_pkey PRIMARY KEY (id);


--
-- Name: player_aliases player_aliases_platform_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_aliases
    ADD CONSTRAINT player_aliases_platform_name_key UNIQUE (platform, name);


--
-- Name: player_profiles player_profiles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_profiles
    ADD CONSTRAINT player_profiles_pkey PRIMARY KEY (id);


--
-- Name: position_filters position_filters_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.position_filters
    ADD CONSTRAINT position_filters_pkey PRIMARY KEY (id);


--
-- Name: position_game_plans position_game_plans_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.position_game_plans
    ADD CONSTRAINT position_game_plans_pkey PRIMARY KEY (position_uuid, classifier);


--
-- Name: positions positions_match_uuid_ordinal_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.positions
    ADD CONSTRAINT positions_match_uuid_ordinal_key UNIQUE (match_uuid, ordinal);


--
-- Name: positions positions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.positions
    ADD CONSTRAINT positions_pkey PRIMARY KEY (id);


--
-- Name: review_hit_sets review_hit_sets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.review_hit_sets
    ADD CONSTRAINT review_hit_sets_pkey PRIMARY KEY (match_id, analysis_id);


--
-- Name: review_hits review_hits_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.review_hits
    ADD CONSTRAINT review_hits_pkey PRIMARY KEY (match_id, analysis_id, game_no, decision_no, checker, id);


--
-- Name: review_sessions review_sessions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.review_sessions
    ADD CONSTRAINT review_sessions_pkey PRIMARY KEY (id);


--
-- Name: saved_positions saved_positions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.saved_positions
    ADD CONSTRAINT saved_positions_pkey PRIMARY KEY (position_uuid);


--
-- Name: score_practice_profiles score_practice_profiles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.score_practice_profiles
    ADD CONSTRAINT score_practice_profiles_pkey PRIMARY KEY (id);


--
-- Name: similar_practice_summaries similar_practice_summaries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.similar_practice_summaries
    ADD CONSTRAINT similar_practice_summaries_pkey PRIMARY KEY (id);


--
-- Name: study_report_snapshots study_report_snapshots_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.study_report_snapshots
    ADD CONSTRAINT study_report_snapshots_pkey PRIMARY KEY (id);


--
-- Name: study_report_snapshots study_report_snapshots_session_id_game_no_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.study_report_snapshots
    ADD CONSTRAINT study_report_snapshots_session_id_game_no_key UNIQUE (session_id, game_no);


--
-- Name: transfer_practice_seen transfer_practice_seen_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.transfer_practice_seen
    ADD CONSTRAINT transfer_practice_seen_pkey PRIMARY KEY (signature);


--
-- Name: virtual_player_members virtual_player_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.virtual_player_members
    ADD CONSTRAINT virtual_player_members_pkey PRIMARY KEY (profile_id);


--
-- Name: virtual_player_profiles virtual_player_profiles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.virtual_player_profiles
    ADD CONSTRAINT virtual_player_profiles_pkey PRIMARY KEY (id);


--
-- Name: anki_auto_sync_due; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX anki_auto_sync_due ON public.anki_auto_sync USING btree (collection_key, next_attempt_at, deck_id);


--
-- Name: anki_sync_history_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX anki_sync_history_recent ON public.anki_sync_history USING btree (deck_id, finished_at DESC, id DESC);


--
-- Name: annotations_game; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX annotations_game ON public.annotations USING btree (game_uuid) WHERE (game_uuid IS NOT NULL);


--
-- Name: annotations_match; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX annotations_match ON public.annotations USING btree (match_uuid) WHERE (match_uuid IS NOT NULL);


--
-- Name: annotations_move; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX annotations_move ON public.annotations USING btree (move_uuid) WHERE (move_uuid IS NOT NULL);


--
-- Name: annotations_position; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX annotations_position ON public.annotations USING btree (position_uuid) WHERE (position_uuid IS NOT NULL);


--
-- Name: annotations_search_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX annotations_search_trgm ON public.annotations USING gin (search_text public.gin_trgm_ops);


--
-- Name: annotations_tags; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX annotations_tags ON public.annotations USING gin (tags);


--
-- Name: benchmark_one_running; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX benchmark_one_running ON public.benchmark_runs USING btree ((true)) WHERE (status = 'running'::text);


--
-- Name: benchmark_runs_history; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX benchmark_runs_history ON public.benchmark_runs USING btree (created_at DESC, queue_no DESC);


--
-- Name: benchmark_runs_queue; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX benchmark_runs_queue ON public.benchmark_runs USING btree (created_at, queue_no) WHERE (status = 'queued'::text);


--
-- Name: card_reviews_history; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX card_reviews_history ON public.card_reviews USING btree (card_id, reviewed_at);


--
-- Name: card_topics_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX card_topics_name ON public.card_topics USING btree (lower(name));


--
-- Name: databases_single_default; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX databases_single_default ON public.databases USING btree (is_default) WHERE is_default;


--
-- Name: decision_index_player_group; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX decision_index_player_group ON public.decision_index USING btree (player_name, game_plan, mistake_kind);


--
-- Name: decision_index_player_loss; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX decision_index_player_loss ON public.decision_index USING btree (player_name, equity_loss DESC, match_id, game_no, decision_no);


--
-- Name: deck_cards_card; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX deck_cards_card ON public.deck_cards USING btree (card_id);


--
-- Name: engine_match_runs_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX engine_match_runs_created ON public.engine_match_runs USING btree (created_at DESC, id DESC);


--
-- Name: engine_match_runs_one_worker; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX engine_match_runs_one_worker ON public.engine_match_runs USING btree ((true)) WHERE (status = 'running'::text);


--
-- Name: engine_match_runs_queue; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX engine_match_runs_queue ON public.engine_match_runs USING btree (created_at, id) WHERE (status = 'queued'::text);


--
-- Name: imported_analyses_move; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX imported_analyses_move ON public.imported_analyses USING btree (match_uuid, game_no, source_move_no);


--
-- Name: imported_analyses_position; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX imported_analyses_position ON public.imported_analyses USING btree (position_uuid) WHERE (position_uuid IS NOT NULL);


--
-- Name: job_events_replay; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX job_events_replay ON public.job_events USING btree (job_id, id);


--
-- Name: jobs_browser_position_ready; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX jobs_browser_position_ready ON public.jobs USING btree (position_uuid, plies DESC, created_at DESC, id DESC) WHERE ((kind = 'analysis'::text) AND (status = 'completed'::text) AND (result IS NOT NULL));


--
-- Name: jobs_catalog_match; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX jobs_catalog_match ON public.jobs USING btree (match_uuid, created_at DESC, id DESC) WHERE (kind = 'match'::text);


--
-- Name: jobs_catalog_position; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX jobs_catalog_position ON public.jobs USING btree (position_uuid, created_at DESC, id DESC) WHERE ((kind = 'analysis'::text) AND (status = 'completed'::text) AND (result IS NOT NULL));


--
-- Name: jobs_claim; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX jobs_claim ON public.jobs USING btree (status, created_at);


--
-- Name: jobs_engine_match_current; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX jobs_engine_match_current ON public.jobs USING btree (match_uuid, COALESCE((engine_snapshot ->> 'engine_id'::text), 'gnubg'::text), source_revision, plies DESC, created_at DESC, id DESC) WHERE ((kind = 'match'::text) AND (status = 'completed'::text));


--
-- Name: jobs_engine_position_ready; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX jobs_engine_position_ready ON public.jobs USING btree (position_uuid, COALESCE((engine_snapshot ->> 'engine_id'::text), 'gnubg'::text), plies DESC, created_at DESC, id DESC) WHERE ((kind = 'analysis'::text) AND (status = 'completed'::text) AND (result IS NOT NULL));


--
-- Name: jobs_match_history; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX jobs_match_history ON public.jobs USING btree (match_uuid, COALESCE(game_no, 0), created_at DESC, id DESC) WHERE (kind = 'match'::text);


--
-- Name: jobs_one_active; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX jobs_one_active ON public.jobs USING btree (position_uuid, kind, plies, engine_config) WHERE (status = ANY (ARRAY['queued'::text, 'running'::text]));


--
-- Name: jobs_one_active_match; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX jobs_one_active_match ON public.jobs USING btree (match_uuid, COALESCE(game_no, 0), plies, engine_config) WHERE ((kind = 'match'::text) AND (status = ANY (ARRAY['queued'::text, 'running'::text])));


--
-- Name: jobs_recurring_current; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX jobs_recurring_current ON public.jobs USING btree (match_uuid, source_revision, plies DESC, created_at DESC, id DESC) WHERE ((kind = 'match'::text) AND (status = 'completed'::text));


--
-- Name: learning_card_game_plan_filter; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX learning_card_game_plan_filter ON public.learning_card_game_plans USING btree (classifier, game_plan, card_id);


--
-- Name: learning_cards_due; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX learning_cards_due ON public.learning_cards USING btree (due_at, id);


--
-- Name: learning_cards_tags; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX learning_cards_tags ON public.learning_cards USING gin (tags);


--
-- Name: match_assessments_queue; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX match_assessments_queue ON public.match_assessments USING btree (created_at, id) WHERE (status = ANY (ARRAY['queued'::text, 'running'::text]));


--
-- Name: match_moves_position; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX match_moves_position ON public.match_moves USING btree (position_uuid) WHERE (position_uuid IS NOT NULL);


--
-- Name: match_players_alias; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX match_players_alias ON public.match_players USING btree (alias_id, match_id);


--
-- Name: matches_black_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_black_trgm ON public.matches USING gin (lower(black) public.gin_trgm_ops);


--
-- Name: matches_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_created ON public.matches USING btree (created_at, id);


--
-- Name: matches_database_black_sort; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_database_black_sort ON public.matches USING btree (database_id, lower(black), id);


--
-- Name: matches_database_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_database_created ON public.matches USING btree (database_id, created_at, id);


--
-- Name: matches_database_length; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_database_length ON public.matches USING btree (database_id, match_length, id);


--
-- Name: matches_database_name; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_database_name ON public.matches USING btree (database_id, lower(COALESCE(NULLIF(display_name, ''::text), filename)), id);


--
-- Name: matches_database_played; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_database_played ON public.matches USING btree (database_id, played_on, id);


--
-- Name: matches_database_players; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_database_players ON public.matches USING btree (database_id, white, black, id);


--
-- Name: matches_database_result; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_database_result ON public.matches USING btree (database_id, result_state, id);


--
-- Name: matches_database_white_sort; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_database_white_sort ON public.matches USING btree (database_id, lower(white), id);


--
-- Name: matches_display_name_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_display_name_trgm ON public.matches USING gin (lower(display_name) public.gin_trgm_ops);


--
-- Name: matches_effective_name_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_effective_name_trgm ON public.matches USING gin (lower(COALESCE(NULLIF(display_name, ''::text), filename)) public.gin_trgm_ops);


--
-- Name: matches_library_database_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_library_database_recent ON public.matches USING btree (database_id, created_at DESC NULLS LAST, id) WHERE (NOT collection_only);


--
-- Name: matches_library_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_library_recent ON public.matches USING btree (created_at DESC NULLS LAST, id) WHERE (NOT collection_only);


--
-- Name: matches_pending_source_metadata; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_pending_source_metadata ON public.matches USING btree (id) WHERE ((NOT source_metadata_loaded) OR native_match_context_pending);


--
-- Name: matches_played; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_played ON public.matches USING btree (played_on, id);


--
-- Name: matches_round_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_round_trgm ON public.matches USING gin (lower(round) public.gin_trgm_ops);


--
-- Name: matches_search_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_search_trgm ON public.matches USING gin (search_text public.gin_trgm_ops);


--
-- Name: matches_tournament_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_tournament_trgm ON public.matches USING gin (lower(tournament) public.gin_trgm_ops);


--
-- Name: matches_white_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX matches_white_trgm ON public.matches USING gin (lower(white) public.gin_trgm_ops);


--
-- Name: mistake_practice_attempts_time; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX mistake_practice_attempts_time ON public.mistake_practice_attempts USING btree (attempted_at DESC);


--
-- Name: move_analyses_errors; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX move_analyses_errors ON public.move_analyses USING btree (job_id, id) WHERE (equity_loss >= (0.04)::double precision);


--
-- Name: move_analyses_game_page; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX move_analyses_game_page ON public.move_analyses USING btree (job_id, game_no, id);


--
-- Name: move_analyses_page; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX move_analyses_page ON public.move_analyses USING btree (job_id, id);


--
-- Name: move_analyses_player; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX move_analyses_player ON public.move_analyses USING btree (job_id, player, id);


--
-- Name: move_analyses_position_checker; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX move_analyses_position_checker ON public.move_analyses USING btree (position_uuid, job_id) WHERE ((kind = 'checker'::text) AND (position_uuid IS NOT NULL));


--
-- Name: play_sessions_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX play_sessions_recent ON public.play_sessions USING btree (updated_at DESC);


--
-- Name: play_sessions_study_reports_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX play_sessions_study_reports_recent ON public.play_sessions USING btree (updated_at DESC, id DESC) WHERE (((state ->> 'phase'::text) = 'match_over'::text) AND (jsonb_typeof((state -> 'review'::text)) = 'object'::text));


--
-- Name: player_aliases_profile; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX player_aliases_profile ON public.player_aliases USING btree (profile_id, id);


--
-- Name: position_filters_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX position_filters_name ON public.position_filters USING btree (lower(name));


--
-- Name: position_game_plans_lookup; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX position_game_plans_lookup ON public.position_game_plans USING btree (classifier, game_plan, position_uuid);


--
-- Name: positions_browser_context; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX positions_browser_context ON public.positions USING btree (source_white_score, source_black_score, source_crawford, source_cube_value, id);


--
-- Name: positions_browser_cube; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX positions_browser_cube ON public.positions USING btree (source_cube_value, id);


--
-- Name: positions_browser_dice; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX positions_browser_dice ON public.positions USING btree (LEAST(dice[1], dice[2]), GREATEST(dice[1], dice[2]), id);


--
-- Name: positions_browser_dice_any; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX positions_browser_dice_any ON public.positions USING gin (dice);


--
-- Name: positions_browser_order; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX positions_browser_order ON public.positions USING btree (match_uuid, ordinal, id);


--
-- Name: positions_match_game_ordinal; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX positions_match_game_ordinal ON public.positions USING btree (match_uuid, game_no, ordinal);


--
-- Name: positions_match_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX positions_match_id ON public.positions USING btree (match_uuid, id);


--
-- Name: positions_position_id; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX positions_position_id ON public.positions USING btree (position_id, id);


--
-- Name: review_sessions_card; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX review_sessions_card ON public.review_sessions USING btree (card_id);


--
-- Name: review_sessions_first_attempt; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX review_sessions_first_attempt ON public.review_sessions USING btree (card_id, card_version) WHERE (attempt_request_id IS NOT NULL);


--
-- Name: score_practice_profiles_expiry; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX score_practice_profiles_expiry ON public.score_practice_profiles USING btree (expires_at);


--
-- Name: similar_practice_summaries_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX similar_practice_summaries_recent ON public.similar_practice_summaries USING btree (completed_at DESC, id DESC);


--
-- Name: study_report_snapshots_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX study_report_snapshots_recent ON public.study_report_snapshots USING btree (updated_at DESC, id DESC);


--
-- Name: virtual_player_members_virtual; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX virtual_player_members_virtual ON public.virtual_player_members USING btree (virtual_id, profile_id);


--
-- Name: virtual_player_profiles_me; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX virtual_player_profiles_me ON public.virtual_player_profiles USING btree (is_me) WHERE is_me;


--
-- Name: virtual_player_profiles_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX virtual_player_profiles_name ON public.virtual_player_profiles USING btree (lower(name));


--
-- Name: annotations annotations_search_text; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER annotations_search_text BEFORE INSERT OR UPDATE OF comment, tags ON public.annotations FOR EACH ROW EXECUTE FUNCTION public.annotations_search_text();


--
-- Name: matches catalog_match_players; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER catalog_match_players AFTER INSERT OR UPDATE OF white, black, collection_only, play_mode, player_source_platform ON public.matches FOR EACH ROW EXECUTE FUNCTION public.catalog_match_players();


--
-- Name: databases databases_protect_system; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER databases_protect_system BEFORE DELETE OR UPDATE OF id, is_default ON public.databases FOR EACH ROW WHEN (old.is_system) EXECUTE FUNCTION public.protect_system_database();


--
-- Name: imported_analyses imported_analyses_drop_review_hits; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER imported_analyses_drop_review_hits AFTER UPDATE ON public.imported_analyses REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION public.review_hits_drop_updated_imported();


--
-- Name: jobs jobs_source_revision; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER jobs_source_revision BEFORE INSERT ON public.jobs FOR EACH ROW EXECUTE FUNCTION public.snapshot_job_source_revision();


--
-- Name: move_analyses move_analyses_drop_decision_index; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER move_analyses_drop_decision_index AFTER UPDATE ON public.move_analyses REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION public.decision_index_drop_updated_analyses();


--
-- Name: move_analyses move_analyses_drop_review_hits; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER move_analyses_drop_review_hits AFTER UPDATE ON public.move_analyses REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION public.review_hits_drop_updated_native();


--
-- Name: matches set_match_player_platform; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER set_match_player_platform BEFORE INSERT OR UPDATE OF player_source_platform ON public.matches FOR EACH ROW EXECUTE FUNCTION public.set_match_player_platform();


--
-- Name: analysis_selections analysis_selections_match_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.analysis_selections
    ADD CONSTRAINT analysis_selections_match_id_fkey FOREIGN KEY (match_id) REFERENCES public.matches(id) ON DELETE CASCADE;


--
-- Name: anki_auto_sync anki_auto_sync_deck_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_auto_sync
    ADD CONSTRAINT anki_auto_sync_deck_id_fkey FOREIGN KEY (deck_id) REFERENCES public.decks(id) ON DELETE CASCADE;


--
-- Name: anki_card_faces anki_card_faces_card_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_card_faces
    ADD CONSTRAINT anki_card_faces_card_id_fkey FOREIGN KEY (card_id) REFERENCES public.learning_cards(id) ON DELETE CASCADE;


--
-- Name: anki_received_reviews anki_received_reviews_card_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_received_reviews
    ADD CONSTRAINT anki_received_reviews_card_id_fkey FOREIGN KEY (card_id) REFERENCES public.learning_cards(id) ON DELETE CASCADE;


--
-- Name: anki_review_exports anki_review_exports_card_id_request_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_review_exports
    ADD CONSTRAINT anki_review_exports_card_id_request_id_fkey FOREIGN KEY (card_id, request_id) REFERENCES public.card_reviews(card_id, request_id) ON DELETE CASCADE;


--
-- Name: anki_source_decks anki_source_decks_deck_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_source_decks
    ADD CONSTRAINT anki_source_decks_deck_id_fkey FOREIGN KEY (deck_id) REFERENCES public.decks(id) ON DELETE CASCADE;


--
-- Name: anki_sync_history anki_sync_history_deck_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.anki_sync_history
    ADD CONSTRAINT anki_sync_history_deck_id_fkey FOREIGN KEY (deck_id) REFERENCES public.decks(id) ON DELETE CASCADE;


--
-- Name: annotations annotations_game_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.annotations
    ADD CONSTRAINT annotations_game_uuid_fkey FOREIGN KEY (game_uuid) REFERENCES public.match_games(id) ON DELETE CASCADE;


--
-- Name: annotations annotations_match_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.annotations
    ADD CONSTRAINT annotations_match_uuid_fkey FOREIGN KEY (match_uuid) REFERENCES public.matches(id) ON DELETE CASCADE;


--
-- Name: annotations annotations_move_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.annotations
    ADD CONSTRAINT annotations_move_uuid_fkey FOREIGN KEY (move_uuid) REFERENCES public.match_moves(id) ON DELETE CASCADE;


--
-- Name: annotations annotations_position_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.annotations
    ADD CONSTRAINT annotations_position_uuid_fkey FOREIGN KEY (position_uuid) REFERENCES public.positions(id) ON DELETE CASCADE;


--
-- Name: card_practice_items card_practice_items_round_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.card_practice_items
    ADD CONSTRAINT card_practice_items_round_id_fkey FOREIGN KEY (round_id) REFERENCES public.card_practice_rounds(id) ON DELETE CASCADE;


--
-- Name: card_reviews card_reviews_card_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.card_reviews
    ADD CONSTRAINT card_reviews_card_id_fkey FOREIGN KEY (card_id) REFERENCES public.learning_cards(id) ON DELETE CASCADE;


--
-- Name: card_reviews card_reviews_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.card_reviews
    ADD CONSTRAINT card_reviews_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.review_sessions(id);


--
-- Name: configuration_state configuration_state_active_profile_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.configuration_state
    ADD CONSTRAINT configuration_state_active_profile_id_fkey FOREIGN KEY (active_profile_id) REFERENCES public.configuration_profiles(id) ON DELETE RESTRICT;


--
-- Name: decision_index decision_index_match_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.decision_index
    ADD CONSTRAINT decision_index_match_id_fkey FOREIGN KEY (match_id) REFERENCES public.decision_index_matches(match_id) ON DELETE CASCADE;


--
-- Name: decision_index_matches decision_index_matches_match_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.decision_index_matches
    ADD CONSTRAINT decision_index_matches_match_id_fkey FOREIGN KEY (match_id) REFERENCES public.matches(id) ON DELETE CASCADE;


--
-- Name: deck_cards deck_cards_card_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.deck_cards
    ADD CONSTRAINT deck_cards_card_id_fkey FOREIGN KEY (card_id) REFERENCES public.learning_cards(id) ON DELETE CASCADE;


--
-- Name: deck_cards deck_cards_deck_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.deck_cards
    ADD CONSTRAINT deck_cards_deck_id_fkey FOREIGN KEY (deck_id) REFERENCES public.decks(id) ON DELETE CASCADE;


--
-- Name: engine_match_results engine_match_results_run_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.engine_match_results
    ADD CONSTRAINT engine_match_results_run_id_fkey FOREIGN KEY (run_id) REFERENCES public.engine_match_runs(id) ON DELETE CASCADE;


--
-- Name: imported_analyses imported_analyses_match_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.imported_analyses
    ADD CONSTRAINT imported_analyses_match_uuid_fkey FOREIGN KEY (match_uuid) REFERENCES public.matches(id) ON DELETE CASCADE;


--
-- Name: imported_analyses imported_analyses_match_uuid_game_no_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.imported_analyses
    ADD CONSTRAINT imported_analyses_match_uuid_game_no_fkey FOREIGN KEY (match_uuid, game_no) REFERENCES public.match_games(match_uuid, game_no) ON DELETE CASCADE;


--
-- Name: imported_analyses imported_analyses_position_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.imported_analyses
    ADD CONSTRAINT imported_analyses_position_uuid_fkey FOREIGN KEY (position_uuid) REFERENCES public.positions(id) ON DELETE SET NULL;


--
-- Name: imported_analysis_projections imported_analysis_projections_imported_analysis_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.imported_analysis_projections
    ADD CONSTRAINT imported_analysis_projections_imported_analysis_id_fkey FOREIGN KEY (imported_analysis_id) REFERENCES public.imported_analyses(id) ON DELETE CASCADE;


--
-- Name: job_events job_events_job_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.job_events
    ADD CONSTRAINT job_events_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.jobs(id) ON DELETE CASCADE;


--
-- Name: jobs jobs_match_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_match_uuid_fkey FOREIGN KEY (match_uuid) REFERENCES public.matches(id) ON DELETE CASCADE;


--
-- Name: jobs jobs_position_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.jobs
    ADD CONSTRAINT jobs_position_uuid_fkey FOREIGN KEY (position_uuid) REFERENCES public.positions(id) ON DELETE CASCADE;


--
-- Name: learning_card_exports learning_card_exports_card_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.learning_card_exports
    ADD CONSTRAINT learning_card_exports_card_id_fkey FOREIGN KEY (card_id) REFERENCES public.learning_cards(id) ON DELETE CASCADE;


--
-- Name: learning_card_game_plans learning_card_game_plans_card_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.learning_card_game_plans
    ADD CONSTRAINT learning_card_game_plans_card_id_fkey FOREIGN KEY (card_id) REFERENCES public.learning_cards(id) ON DELETE CASCADE;


--
-- Name: match_analysis_games match_analysis_games_job_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_analysis_games
    ADD CONSTRAINT match_analysis_games_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.jobs(id) ON DELETE CASCADE;


--
-- Name: match_analysis_timing match_analysis_timing_benchmark_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_analysis_timing
    ADD CONSTRAINT match_analysis_timing_benchmark_id_fkey FOREIGN KEY (benchmark_id) REFERENCES public.benchmark_runs(id) ON DELETE SET NULL;


--
-- Name: match_analysis_timing match_analysis_timing_job_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_analysis_timing
    ADD CONSTRAINT match_analysis_timing_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.jobs(id) ON DELETE CASCADE;


--
-- Name: match_games match_games_match_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_games
    ADD CONSTRAINT match_games_match_uuid_fkey FOREIGN KEY (match_uuid) REFERENCES public.matches(id) ON DELETE CASCADE;


--
-- Name: match_moves match_moves_match_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_moves
    ADD CONSTRAINT match_moves_match_uuid_fkey FOREIGN KEY (match_uuid) REFERENCES public.matches(id) ON DELETE CASCADE;


--
-- Name: match_moves match_moves_match_uuid_game_no_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_moves
    ADD CONSTRAINT match_moves_match_uuid_game_no_fkey FOREIGN KEY (match_uuid, game_no) REFERENCES public.match_games(match_uuid, game_no) ON DELETE CASCADE;


--
-- Name: match_moves match_moves_position_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_moves
    ADD CONSTRAINT match_moves_position_uuid_fkey FOREIGN KEY (position_uuid) REFERENCES public.positions(id) ON DELETE SET NULL;


--
-- Name: match_players match_players_alias_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_players
    ADD CONSTRAINT match_players_alias_id_fkey FOREIGN KEY (alias_id) REFERENCES public.player_aliases(id);


--
-- Name: match_players match_players_match_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.match_players
    ADD CONSTRAINT match_players_match_id_fkey FOREIGN KEY (match_id) REFERENCES public.matches(id) ON DELETE CASCADE;


--
-- Name: matches matches_database_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.matches
    ADD CONSTRAINT matches_database_id_fkey FOREIGN KEY (database_id) REFERENCES public.databases(id);


--
-- Name: move_analyses move_analyses_job_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.move_analyses
    ADD CONSTRAINT move_analyses_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.jobs(id) ON DELETE CASCADE;


--
-- Name: move_analyses move_analyses_position_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.move_analyses
    ADD CONSTRAINT move_analyses_position_uuid_fkey FOREIGN KEY (position_uuid) REFERENCES public.positions(id) ON DELETE CASCADE;


--
-- Name: play_sessions play_sessions_library_match_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.play_sessions
    ADD CONSTRAINT play_sessions_library_match_id_fkey FOREIGN KEY (library_match_id) REFERENCES public.matches(id) ON DELETE SET NULL;


--
-- Name: player_aliases player_aliases_profile_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.player_aliases
    ADD CONSTRAINT player_aliases_profile_id_fkey FOREIGN KEY (profile_id) REFERENCES public.player_profiles(id);


--
-- Name: position_game_plans position_game_plans_position_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.position_game_plans
    ADD CONSTRAINT position_game_plans_position_uuid_fkey FOREIGN KEY (position_uuid) REFERENCES public.positions(id) ON DELETE CASCADE;


--
-- Name: positions positions_match_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.positions
    ADD CONSTRAINT positions_match_uuid_fkey FOREIGN KEY (match_uuid) REFERENCES public.matches(id) ON DELETE CASCADE;


--
-- Name: review_hit_sets review_hit_sets_match_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.review_hit_sets
    ADD CONSTRAINT review_hit_sets_match_id_fkey FOREIGN KEY (match_id) REFERENCES public.matches(id) ON DELETE CASCADE;


--
-- Name: review_hits review_hits_match_id_analysis_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.review_hits
    ADD CONSTRAINT review_hits_match_id_analysis_id_fkey FOREIGN KEY (match_id, analysis_id) REFERENCES public.review_hit_sets(match_id, analysis_id) ON DELETE CASCADE;


--
-- Name: review_sessions review_sessions_card_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.review_sessions
    ADD CONSTRAINT review_sessions_card_id_fkey FOREIGN KEY (card_id) REFERENCES public.learning_cards(id) ON DELETE CASCADE;


--
-- Name: saved_positions saved_positions_position_uuid_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.saved_positions
    ADD CONSTRAINT saved_positions_position_uuid_fkey FOREIGN KEY (position_uuid) REFERENCES public.positions(id) ON DELETE CASCADE;


--
-- Name: score_practice_profiles score_practice_profiles_card_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.score_practice_profiles
    ADD CONSTRAINT score_practice_profiles_card_id_fkey FOREIGN KEY (card_id) REFERENCES public.learning_cards(id) ON DELETE CASCADE;


--
-- Name: study_report_snapshots study_report_snapshots_library_match_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.study_report_snapshots
    ADD CONSTRAINT study_report_snapshots_library_match_id_fkey FOREIGN KEY (library_match_id) REFERENCES public.matches(id) ON DELETE SET NULL;


--
-- Name: study_report_snapshots study_report_snapshots_session_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.study_report_snapshots
    ADD CONSTRAINT study_report_snapshots_session_id_fkey FOREIGN KEY (session_id) REFERENCES public.play_sessions(id) ON DELETE CASCADE;


--
-- Name: virtual_player_members virtual_player_members_profile_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.virtual_player_members
    ADD CONSTRAINT virtual_player_members_profile_id_fkey FOREIGN KEY (profile_id) REFERENCES public.player_profiles(id) ON DELETE CASCADE;


--
-- Name: virtual_player_members virtual_player_members_virtual_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.virtual_player_members
    ADD CONSTRAINT virtual_player_members_virtual_id_fkey FOREIGN KEY (virtual_id) REFERENCES public.virtual_player_profiles(id) ON DELETE CASCADE;


--
-- Seed rows
--

INSERT INTO public.databases (id, name, description, version, is_default, collection_only) VALUES
    ('00000000-0000-0000-0000-000000000001', 'My matches', '', 2, false, false),
    ('00000000-0000-0000-0000-000000000002', 'Saved positions', 'Internal storage for independent saved occurrences', 1, false, true),
    ('00000000-0000-0000-0000-000000000003', 'Bot Matches', '', 1, false, false),
    ('00000000-0000-0000-0000-000000000004', 'Paused Matches', '', 1, false, false),
    ('00000000-0000-0000-0000-000000000005', 'Engine Duels', '', 1, false, false),
    -- The default for new matches; matches.database_id defaults to this id.
    ('81a18d0f-38c1-4db8-81c0-a743adbc4480', 'Unsorted', 'Matches ohne konkrete Zuordnung', 3, true, false);

INSERT INTO public.engine_installations (engine_id) VALUES ('bgsage');

INSERT INTO public.learning_preferences DEFAULT VALUES;
