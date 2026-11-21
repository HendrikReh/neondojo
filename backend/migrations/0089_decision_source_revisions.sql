-- These tokens describe derived read models, so they are recreated on restore.
CREATE TABLE public.native_analysis_revisions (
    job_id uuid PRIMARY KEY REFERENCES public.jobs(id) ON DELETE CASCADE,
    revision text NOT NULL
);

CREATE TABLE public.imported_analysis_revisions (
    match_id uuid PRIMARY KEY REFERENCES public.matches(id) ON DELETE CASCADE,
    revision text NOT NULL
);

CREATE FUNCTION public.mark_native_analysis_revisions(affected uuid[]) RETURNS void
LANGUAGE sql AS $$
    INSERT INTO public.native_analysis_revisions AS revisions(job_id, revision)
    SELECT id, 'tx:' || pg_current_xact_id()::text FROM public.jobs
    WHERE id = ANY (affected)
    ORDER BY id
    ON CONFLICT (job_id) DO UPDATE SET revision = EXCLUDED.revision
    WHERE revisions.revision <> EXCLUDED.revision;
$$;

-- Decision and checkpoint writes in the same transaction share a token.
-- Cascades can remove the parent job first. The insert above skips those jobs.
CREATE FUNCTION public.mark_native_analysis_rows() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        PERFORM public.mark_native_analysis_revisions(ARRAY(SELECT DISTINCT job_id FROM new_rows));
    ELSIF TG_OP = 'DELETE' THEN
        PERFORM public.mark_native_analysis_revisions(ARRAY(SELECT DISTINCT job_id FROM old_rows));
    ELSE
        PERFORM public.mark_native_analysis_revisions(ARRAY(
            SELECT job_id FROM old_rows UNION SELECT job_id FROM new_rows
        ));
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER move_analyses_revision_insert AFTER INSERT ON public.move_analyses
    REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION public.mark_native_analysis_rows();
CREATE TRIGGER move_analyses_revision_update AFTER UPDATE ON public.move_analyses
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION public.mark_native_analysis_rows();
CREATE TRIGGER move_analyses_revision_delete AFTER DELETE ON public.move_analyses
    REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION public.mark_native_analysis_rows();

CREATE TRIGGER match_analysis_games_revision_insert AFTER INSERT ON public.match_analysis_games
    REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION public.mark_native_analysis_rows();
CREATE TRIGGER match_analysis_games_revision_update AFTER UPDATE ON public.match_analysis_games
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION public.mark_native_analysis_rows();
CREATE TRIGGER match_analysis_games_revision_delete AFTER DELETE ON public.match_analysis_games
    REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION public.mark_native_analysis_rows();

-- An explicitly selected job can become eligible for study upon completion
-- with no further changes to its saved decisions or checkpoints.
CREATE FUNCTION public.mark_native_analysis_job() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    PERFORM public.mark_native_analysis_revisions(ARRAY[NEW.id]);
    RETURN NULL;
END;
$$;

CREATE TRIGGER jobs_revision_update AFTER UPDATE OF status, plies, game_no, result, engine_snapshot ON public.jobs
    FOR EACH ROW WHEN (
        NEW.kind = 'match' AND (
            OLD.status IS DISTINCT FROM NEW.status OR
            OLD.plies IS DISTINCT FROM NEW.plies OR
            OLD.game_no IS DISTINCT FROM NEW.game_no OR
            (OLD.result ->> 'engine') IS DISTINCT FROM (NEW.result ->> 'engine') OR
            (OLD.engine_snapshot ->> 'engine_id') IS DISTINCT FROM (NEW.engine_snapshot ->> 'engine_id')
        )
    ) EXECUTE FUNCTION public.mark_native_analysis_job();

CREATE FUNCTION public.mark_imported_analysis_rows() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    affected uuid[];
BEGIN
    IF TG_OP = 'INSERT' THEN
        affected := ARRAY(SELECT DISTINCT match_uuid FROM new_rows);
    ELSIF TG_OP = 'DELETE' THEN
        affected := ARRAY(SELECT DISTINCT match_uuid FROM old_rows);
    ELSE
        affected := ARRAY(SELECT match_uuid FROM old_rows UNION SELECT match_uuid FROM new_rows);
    END IF;
    INSERT INTO public.imported_analysis_revisions AS revisions(match_id, revision)
    SELECT id, 'tx:' || pg_current_xact_id()::text FROM public.matches
    WHERE id = ANY (affected)
    ORDER BY id
    ON CONFLICT (match_id) DO UPDATE SET revision = EXCLUDED.revision
    WHERE revisions.revision <> EXCLUDED.revision;
    RETURN NULL;
END;
$$;

CREATE TRIGGER imported_analyses_revision_insert AFTER INSERT ON public.imported_analyses
    REFERENCING NEW TABLE AS new_rows FOR EACH STATEMENT EXECUTE FUNCTION public.mark_imported_analysis_rows();
CREATE TRIGGER imported_analyses_revision_update AFTER UPDATE ON public.imported_analyses
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION public.mark_imported_analysis_rows();
CREATE TRIGGER imported_analyses_revision_delete AFTER DELETE ON public.imported_analyses
    REFERENCING OLD TABLE AS old_rows FOR EACH STATEMENT EXECUTE FUNCTION public.mark_imported_analysis_rows();

-- A search for the previous player no longer finds this match in its freshness
-- scope. Remove those cached rows at the same time as the participant change.
CREATE FUNCTION public.invalidate_decision_index_match_metadata() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM public.decision_index_matches WHERE match_id = NEW.id;
    RETURN NULL;
END;
$$;

CREATE TRIGGER matches_invalidate_decision_index AFTER UPDATE OF white, black, played_on, created_at ON public.matches
    FOR EACH ROW WHEN (
        OLD.white IS DISTINCT FROM NEW.white OR OLD.black IS DISTINCT FROM NEW.black OR
        OLD.played_on IS DISTINCT FROM NEW.played_on OR OLD.created_at IS DISTINCT FROM NEW.created_at
    ) EXECUTE FUNCTION public.invalidate_decision_index_match_metadata();

DROP TRIGGER move_analyses_drop_decision_index ON public.move_analyses;
DROP FUNCTION public.decision_index_drop_updated_analyses();

-- Preserve existing current fingerprints while leaving already-stale entries
-- stale. The first future write replaces this legacy token.
INSERT INTO public.native_analysis_revisions(job_id, revision)
SELECT job_id, count(*)::text || ':' || sum(id)::text
FROM public.move_analyses
GROUP BY job_id;

-- Give legacy sources a row publishers can lock, preserving their fingerprints.
INSERT INTO public.native_analysis_revisions(job_id, revision)
SELECT DISTINCT job_id, '0:0' FROM public.match_analysis_games
ON CONFLICT (job_id) DO NOTHING;

INSERT INTO public.imported_analysis_revisions(match_id, revision)
SELECT DISTINCT match_uuid, '' FROM public.imported_analyses;

-- Earlier versions did not invalidate a projection after a date edit.
UPDATE public.decision_index d SET played_on = coalesce(m.played_on, m.created_at::date)
FROM public.matches m
WHERE d.match_id = m.id AND d.played_on IS DISTINCT FROM coalesce(m.played_on, m.created_at::date);
