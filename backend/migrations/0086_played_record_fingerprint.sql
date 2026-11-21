-- The duplicate match finder compares the played record of library matches.
-- Two matches have the same played record when every game has the same
-- sequence of checker decisions: the board (Position ID), the roll as an
-- unordered pair, the cube value, the cube owner relative to the player on
-- roll, Crawford and the score as own and opposing score. The record is
-- therefore independent of which side a source calls White and of the order
-- in which it records the dice. A match length is compared separately.
--
-- This function is the one definition of that record. It fingerprints a set
-- of matches in one statement; matches without positions are not returned.
-- A match context that cannot be decoded falls back to the raw GNUbg Match
-- ID, which keeps the record complete instead of dropping that position.
CREATE FUNCTION played_record_fingerprints(target_matches uuid[])
    RETURNS TABLE(match_uuid uuid, fingerprint bytea)
    LANGUAGE sql STABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog', 'public'
    AS $$
SELECT p.match_uuid,
       sha256(convert_to(string_agg(
           p.game_no::text || ' ' || p.position_id || ' ' ||
           CASE WHEN p.source_cube_value IS NULL THEN p.gnubg_match_id ELSE
               least(p.dice[1], p.dice[2])::text || greatest(p.dice[1], p.dice[2])::text
               || ' ' || p.source_cube_value::text || ' ' ||
               CASE WHEN p.source_cube_owner = -1 THEN 'c'
                    WHEN p.source_cube_owner = p.player THEN 'r' ELSE 'o' END
               || ' ' || p.source_crawford::int::text || ' ' ||
               CASE WHEN p.player = 0
                    THEN p.source_white_score::text || ' ' || p.source_black_score::text
                    ELSE p.source_black_score::text || ' ' || p.source_white_score::text END
           END,
           ',' ORDER BY p.ordinal), 'UTF8'))
FROM (SELECT DISTINCT id FROM unnest(target_matches) AS t(id)) t
JOIN public.positions p ON p.match_uuid = t.id
GROUP BY p.match_uuid
$$;

CREATE FUNCTION played_record_fingerprint(target_match uuid) RETURNS bytea
    LANGUAGE sql STABLE PARALLEL SAFE
    SET search_path TO 'pg_catalog', 'public'
    AS $$
SELECT fingerprint FROM played_record_fingerprints(ARRAY[target_match])
$$;

-- Stored for library matches; NULL for saved-position context and for a match
-- without positions.
ALTER TABLE matches ADD COLUMN played_record_fingerprint bytea;

-- Store the played record of the given library matches, writing only changed
-- values.
CREATE FUNCTION store_played_record_fingerprints(target_matches uuid[]) RETURNS void
    LANGUAGE sql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
UPDATE public.matches m
SET played_record_fingerprint = f.fingerprint
FROM (
    SELECT t.id, r.fingerprint
    FROM (SELECT DISTINCT id FROM unnest(target_matches) AS t(id)) t
    LEFT JOIN played_record_fingerprints(target_matches) r ON r.match_uuid = t.id
) f
WHERE m.id = f.id
  AND NOT m.collection_only
  AND m.played_record_fingerprint IS DISTINCT FROM f.fingerprint
$$;

-- Every write to positions refreshes the matches it touched, once per
-- statement: imports, copies, archived play sessions, retention and restores.
CREATE FUNCTION refresh_played_record_fingerprints() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'pg_catalog', 'public'
    AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        PERFORM store_played_record_fingerprints(ARRAY(SELECT DISTINCT match_uuid FROM new_positions));
    ELSIF TG_OP = 'UPDATE' THEN
        PERFORM store_played_record_fingerprints(ARRAY(
            SELECT match_uuid FROM new_positions UNION SELECT match_uuid FROM old_positions));
    ELSE
        PERFORM store_played_record_fingerprints(ARRAY(SELECT DISTINCT match_uuid FROM old_positions));
    END IF;
    RETURN NULL;
END $$;

CREATE TRIGGER positions_played_record_insert AFTER INSERT ON positions
    REFERENCING NEW TABLE AS new_positions
    FOR EACH STATEMENT EXECUTE FUNCTION refresh_played_record_fingerprints();
CREATE TRIGGER positions_played_record_update AFTER UPDATE ON positions
    REFERENCING OLD TABLE AS old_positions NEW TABLE AS new_positions
    FOR EACH STATEMENT EXECUTE FUNCTION refresh_played_record_fingerprints();
CREATE TRIGGER positions_played_record_delete AFTER DELETE ON positions
    REFERENCING OLD TABLE AS old_positions
    FOR EACH STATEMENT EXECUTE FUNCTION refresh_played_record_fingerprints();

SELECT store_played_record_fingerprints(ARRAY(SELECT id FROM matches WHERE NOT collection_only));
