-- Legacy BGTSC exports omitted the profile's cubeless flag. Their catalog
-- labeled every checker evaluation cubeful regardless of the game's rules.
CREATE FUNCTION repair_legacy_retained_checker_labels(target_match uuid) RETURNS void
LANGUAGE sql AS $$
    WITH repaired AS (
        UPDATE imported_analyses a
        SET data = a.data || jsonb_build_object(
            'depth', (a.data->>'depth_plies') || '-ply cubeless',
            'candidates', (
                SELECT jsonb_agg(candidate || jsonb_build_object(
                    'equity_kind', 'cubeless',
                    'depth', (a.data->>'depth_plies') || '-ply cubeless'
                ) ORDER BY ordinal)
                FROM jsonb_array_elements(a.data->'candidates') WITH ORDINALITY c(candidate, ordinal)
            )
        )
        FROM match_games g
        WHERE g.match_uuid = a.match_uuid AND g.game_no = a.game_no
          AND (target_match IS NULL OR g.match_uuid = target_match)
          AND g.cube_enabled = false
          AND a.kind = 'checker'
          AND a.data->>'engine' = 'NeonGammon'
          AND a.data->>'method' = 'evaluation'
          AND a.data->'raw'->>'version' = '1'
          AND a.data->'raw' ? 'native_id'
          AND NOT (a.data->'raw' ? 'cubeless')
          AND a.data->>'depth' = (a.data->>'depth_plies') || '-ply cubeful'
          AND jsonb_typeof(a.data->'candidates') = 'array'
        RETURNING a.id
    )
    DELETE FROM imported_analysis_projections
    WHERE imported_analysis_id IN (SELECT id FROM repaired);
$$;

SELECT repair_legacy_retained_checker_labels(NULL);
