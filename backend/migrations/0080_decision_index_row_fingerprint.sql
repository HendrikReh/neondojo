-- The decision index fingerprint no longer hashes every saved decision's JSON.
-- It now ends with the count and the sum of the ids of the effective job's
-- move_analyses rows. Ids come from a sequence and are never reused, so every
-- insert or delete changes that part, and the move_analyses_page index answers
-- it without reading the JSON.

-- Updates keep both values, so drop the index entries of every match whose
-- saved decisions an UPDATE touches. Data migrations and corrections then
-- re-index those matches on their next read.
CREATE OR REPLACE FUNCTION decision_index_drop_updated_analyses() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    DELETE FROM decision_index_matches x
    USING jobs j
    WHERE x.match_id = j.match_uuid
      AND j.id IN (SELECT job_id FROM old_rows UNION SELECT job_id FROM new_rows);
    RETURN NULL;
END
$$;

DROP TRIGGER IF EXISTS move_analyses_drop_decision_index ON move_analyses;
CREATE TRIGGER move_analyses_drop_decision_index
    AFTER UPDATE ON move_analyses
    REFERENCING OLD TABLE AS old_rows NEW TABLE AS new_rows
    FOR EACH STATEMENT EXECUTE FUNCTION decision_index_drop_updated_analyses();

-- Carry current entries over to the new format, so that the upgrade does not
-- re-index every match at once. A request re-indexes at most 400 matches, so a
-- full invalidation would leave large players unsearchable across all
-- databases. The fourth field is the effective job id ('' without one); the
-- format version and the analysis id before it contain no '|'. Only an entry
-- whose final field still equals the hash of that job's current decisions is
-- rewritten. Any other entry was already stale and is rebuilt as before.
WITH entries AS (
    SELECT x.match_id, x.fingerprint,
           CASE WHEN split_part(x.fingerprint, '|', 4)
                     ~ '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                THEN split_part(x.fingerprint, '|', 4)::uuid END AS job_id
    FROM decision_index_matches x
), parts AS (
    SELECT e.match_id, e.fingerprint,
           (SELECT coalesce(sum(hashtextextended(a.data::text, 0)), 0)::text
            FROM move_analyses a WHERE a.job_id = e.job_id) AS hashed,
           (SELECT count(*)::text || ':' || coalesce(sum(a.id), 0)::text
            FROM move_analyses a WHERE a.job_id = e.job_id) AS counted
    FROM entries e
)
UPDATE decision_index_matches x
SET fingerprint = left(p.fingerprint, length(p.fingerprint) - length(p.hashed)) || p.counted
FROM parts p
WHERE x.match_id = p.match_id
  AND right(p.fingerprint, length(p.hashed) + 1) = '|' || p.hashed;
