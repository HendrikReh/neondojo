-- pg_restore clears the session search_path when it loads generated columns.
ALTER FUNCTION public.position_browser_context(text)
    SET search_path TO pg_catalog, public;

-- Recompute valid contexts that were lost when restoring an older dump.
UPDATE public.positions
SET gnubg_match_id = gnubg_match_id
WHERE source_cube_value IS NULL
  AND public.position_browser_context(gnubg_match_id) IS NOT NULL;
