-- The duplicate match finder (duplicate_matches.rs) groups library matches by
-- match length and played record. This index holds exactly those matches in
-- group order, with the columns the groups read, so the finder no longer
-- scans the matches table.
CREATE INDEX matches_played_record_idx
    ON public.matches (match_length, played_record_fingerprint, match_no)
    INCLUDE (id, sha256, database_id)
    WHERE played_record_fingerprint IS NOT NULL AND NOT collection_only;

-- Claims, lease recovery and the active-job checks only look for queued and
-- running jobs. The former index also held every finished job.
CREATE INDEX jobs_active_claim
    ON public.jobs (status, created_at)
    WHERE status IN ('queued', 'running');
DROP INDEX public.jobs_claim;

-- Progress events only matter while a job is queued or running: the event
-- stream of a finished job ends with its completed or failed event, which
-- carries the final progress. When a job finishes, remove its progress rows
-- in the same transaction. Queued, running, completed and failed events stay.
CREATE FUNCTION public.prune_finished_job_progress() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path = pg_catalog
AS $$
BEGIN
    DELETE FROM public.job_events WHERE job_id = NEW.id AND event = 'progress';
    RETURN NULL;
END;
$$;

CREATE TRIGGER jobs_prune_finished_progress
    AFTER UPDATE OF status ON public.jobs
    FOR EACH ROW
    WHEN (OLD.status IN ('queued', 'running') AND NEW.status IN ('completed', 'failed'))
    EXECUTE FUNCTION public.prune_finished_job_progress();

-- Apply the same rule once to the jobs that finished before this migration.
DELETE FROM public.job_events e
USING public.jobs j
WHERE j.id = e.job_id
  AND j.status IN ('completed', 'failed')
  AND e.event = 'progress';
