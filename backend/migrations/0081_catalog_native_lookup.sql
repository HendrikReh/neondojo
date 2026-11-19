-- The effective analysis and the decision index read only the identity of
-- each match's native analysis. The native decision counts used to gate the
-- row, so every such read counted all saved decisions of each job. Decide
-- eligibility with a direct existence check and keep the counts as an optional
-- join that the planner drops when no caller reads them, as migration 0074
-- does for imported analyses. A job has a positive count exactly when one of
-- its decisions belongs to a checkpointed game.
CREATE OR REPLACE VIEW analysis_catalog_sources AS
SELECT m.id AS match_id,'job:'||j.id::text AS analysis_id,'native'::text AS source,
       CASE WHEN j.game_no IS NULL THEN 'match' ELSE 'game' END AS scope,
       j.id AS job_id,j.created_at,j.status,
       (j.status='completed' AND j.completed_games=j.total_games
        AND checkpoints.games>=j.total_games AND NOT m.collection_only) AS complete,
       counts.checker_count,counts.cube_count,
       coalesce(nullif(j.result->>'engine',''),
           CASE WHEN j.engine_snapshot->>'engine_id'='bgsage' THEN 'Open Sage' ELSE 'GNU Backgammon' END) AS engine,
       j.plies::text||'-ply cubeful' AS depth,j.game_no,NULL::uuid AS position_id
FROM matches m JOIN jobs j ON j.match_uuid=m.id AND j.source_revision=m.source_revision AND j.kind='match'
JOIN LATERAL (SELECT count(*) AS games FROM match_analysis_games c WHERE c.job_id=j.id
    AND (j.game_no IS NULL OR c.game_no=j.game_no)) checkpoints ON checkpoints.games>0
LEFT JOIN LATERAL (
    SELECT count(*) FILTER(WHERE a.kind='checker') AS checker_count,
           count(*) FILTER(WHERE a.kind='cube') AS cube_count
    FROM move_analyses a JOIN match_analysis_games c ON c.job_id=a.job_id AND c.game_no=a.game_no
    WHERE a.job_id=j.id AND (j.game_no IS NULL OR a.game_no=j.game_no)
) counts ON true
WHERE j.status IN ('completed','running','failed')
AND (EXISTS (
    SELECT 1 FROM move_analyses a JOIN match_analysis_games c ON c.job_id=a.job_id AND c.game_no=a.game_no
    WHERE a.job_id=j.id AND (j.game_no IS NULL OR a.game_no=j.game_no)) OR EXISTS (
    SELECT 1 FROM match_analysis_games c WHERE c.job_id=j.id
    AND (j.game_no IS NULL OR c.game_no=j.game_no)
    AND c.summary->>'completed_games'='1' AND c.summary->>'total_games'='1'))
AND NOT EXISTS (
    SELECT 1 FROM play_sessions live WHERE live.library_match_id=m.id
    AND coalesce(live.state->>'play_mode','competition')='competition'
    AND live.state->>'phase'<>'match_over'
    AND NOT (live.state->>'match_length'='0' AND live.state->>'phase'='game_over'))
UNION ALL
SELECT m.id,'job:'||j.id::text,'native','position',j.id,j.created_at,j.status,true,
       1::bigint,0::bigint,coalesce(nullif(j.result->>'engine',''),
           CASE WHEN j.engine_snapshot->>'engine_id'='bgsage' THEN 'Open Sage' ELSE 'GNU Backgammon' END),
       j.plies::text||'-ply',p.game_no,p.id
FROM matches m JOIN positions p ON p.match_uuid=m.id
JOIN jobs j ON j.position_uuid=p.id AND j.kind='analysis' AND j.source_revision=m.source_revision
WHERE j.status='completed' AND jsonb_typeof(j.result->'candidates')='array'
    AND jsonb_array_length(CASE WHEN jsonb_typeof(j.result->'candidates')='array' THEN j.result->'candidates' ELSE '[]'::jsonb END)>0 AND NOT EXISTS (
    SELECT 1 FROM play_sessions live WHERE live.library_match_id=m.id
    AND coalesce(live.state->>'play_mode','competition')='competition'
    AND live.state->>'phase'<>'match_over'
    AND NOT (live.state->>'match_length'='0' AND live.state->>'phase'='game_over'))
UNION ALL
SELECT m.id,'imported:'||m.id::text,'imported','match',NULL::uuid,m.created_at,'completed',false,
       imported.checker_count,imported.cube_count,imported.engine,imported.depth,NULL::integer,NULL::uuid
FROM matches m LEFT JOIN LATERAL (
    SELECT count(*) FILTER(WHERE a.kind='checker') AS checker_count,
           count(*) FILTER(WHERE a.kind='cube') AS cube_count,
           coalesce(string_agg(DISTINCT nullif(concat_ws(' ',a.data->>'engine',a.data->>'engine_version'),''),', '
               ORDER BY nullif(concat_ws(' ',a.data->>'engine',a.data->>'engine_version'),'')),'Imported') AS engine,
           string_agg(DISTINCT a.data->>'depth',', ' ORDER BY a.data->>'depth') AS depth
    FROM primary_imported_analyses a WHERE a.match_uuid=m.id AND a.kind IN ('checker','cube')
        AND imported_analysis_has_values(a.data)
) imported ON true
WHERE EXISTS (
    SELECT 1 FROM imported_analyses a WHERE a.match_uuid=m.id AND a.kind IN ('checker','cube')
    AND imported_analysis_has_values(a.data))
AND NOT EXISTS (
    SELECT 1 FROM play_sessions live WHERE live.library_match_id=m.id
    AND coalesce(live.state->>'play_mode','competition')='competition'
    AND live.state->>'phase'<>'match_over'
    AND NOT (live.state->>'match_length'='0' AND live.state->>'phase'='game_over'));
