UPDATE match_games g SET cube_enabled = NULL
FROM matches m LEFT JOIN matches origin ON m.collection_only AND origin.id=m.source_match_id
WHERE m.id = g.match_uuid
AND (strpos(coalesce(origin.sgf, m.sgf), 'NoCube') > 0
     OR strpos(coalesce(origin.sgf, m.sgf), chr(92)) > 0);
