-- NULL means that the runtime backfill must read the game's immutable SGF.
-- An escape can split a property value, including the literal NoCube token.
ALTER TABLE match_games ADD COLUMN cube_enabled boolean DEFAULT true;
CREATE FUNCTION mark_legacy_game_cube_rules() RETURNS void LANGUAGE sql AS $$
    UPDATE match_games g SET cube_enabled = NULL
    FROM matches m LEFT JOIN matches origin ON m.collection_only AND origin.id=m.source_match_id
    WHERE m.id=g.match_uuid AND (
        (m.collection_only AND origin.id IS NULL)
        OR strpos(coalesce(origin.sgf, m.sgf), 'NoCube') > 0
        OR strpos(coalesce(origin.sgf, m.sgf), chr(92)) > 0
    );
$$;
SELECT mark_legacy_game_cube_rules();
CREATE INDEX match_games_pending_cube_rules ON match_games(match_uuid) WHERE cube_enabled IS NULL;
