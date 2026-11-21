CREATE FUNCTION public.notify_job_events() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path = pg_catalog
AS $$
BEGIN
    PERFORM pg_notify('neondojo_job_events', job_id::text)
    FROM (SELECT DISTINCT job_id FROM inserted_job_events) AS jobs;
    RETURN NULL;
END;
$$;

CREATE TRIGGER job_events_notify
    AFTER INSERT ON public.job_events
    REFERENCING NEW TABLE AS inserted_job_events
    FOR EACH STATEMENT EXECUTE FUNCTION public.notify_job_events();
