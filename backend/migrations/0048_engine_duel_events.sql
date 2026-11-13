-- Match the archived SGF bytes so moved or copied duels are found as well.
WITH series_events AS (
    SELECT r.id,
           'Duel ' || string_agg(
               (CASE side.profile->>'engine_id'
                   WHEN 'gnubg' THEN 'GNUbg'
                   WHEN 'bgsage' THEN 'Open Sage'
               END) || ' ' || (side.profile->>'plies') || '-ply',
               ' vs. ' ORDER BY side.ordinal
           ) AS event
    FROM engine_match_runs r
    CROSS JOIN LATERAL (VALUES (1, r.configuration->'a'), (2, r.configuration->'b'))
        AS side(ordinal, profile)
    WHERE side.profile->>'engine_id' IN ('gnubg', 'bgsage')
      AND side.profile->>'plies' ~ '^[0-4]$'
    GROUP BY r.id
    HAVING count(*) = 2
)
UPDATE matches m
SET tournament = e.event,
    metadata_version = m.metadata_version + 1
FROM engine_match_results d
JOIN series_events e ON e.id = d.run_id
WHERE m.sha256 = encode(sha256(convert_to(d.sgf, 'UTF8')), 'hex')
  AND m.format = 'sgf'
  AND NOT m.collection_only
  AND m.tournament IS NULL;
