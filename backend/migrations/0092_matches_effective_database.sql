-- Saved positions and imported-analysis projections filter matches by the
-- database a match belongs to, or for a collection-only copy, the database it
-- was saved from.
CREATE INDEX matches_effective_database_idx
    ON public.matches ((coalesce(source_database_id, database_id)), id);
