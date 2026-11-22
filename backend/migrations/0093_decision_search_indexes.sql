-- Faster decision search and recurring mistakes on large libraries. Every
-- request first checks that the decision index is current for one player's
-- matches; that check and the search itself read these indexes. Only
-- indexes, a view and a composite type change, so earlier backups stay
-- compatible.

-- The player scope compares names without case and surrounding spaces.
-- Saved-position collections are never in that scope.
CREATE INDEX matches_library_white_player ON public.matches (lower(btrim(white)))
    WHERE NOT collection_only;
CREATE INDEX matches_library_black_player ON public.matches (lower(btrim(black)))
    WHERE NOT collection_only;

-- Older foreign imports whose move and analysis catalogs are still missing.
-- Usually empty, so the legacy-catalog step costs one index probe.
CREATE INDEX matches_pending_imported_catalog ON public.matches (id)
    WHERE format IN ('xg', 'bgf', 'sgf')
      AND (NOT imported_analysis_loaded OR NOT moves_cataloged);

-- The same choice as before, evaluated in priority order. Without an explicit
-- choice, a complete native match analysis wins, then the imported analysis,
-- then the newest of the other completed analyses. COALESCE evaluates a later
-- group only when the earlier ones have no analysis, so an analyzed match no
-- longer reads the analyses of each of its positions.
CREATE TYPE public.analysis_choice AS (analysis_id text, job_id uuid, source text);

CREATE OR REPLACE VIEW public.effective_analysis_selections AS
SELECT m.id AS match_id,
    (chosen.choice).analysis_id AS analysis_id,
    (chosen.choice).job_id AS job_id,
    (chosen.choice).source AS source
FROM public.matches m
LEFT JOIN public.analysis_selections selected ON selected.match_id = m.id
CROSS JOIN LATERAL (
    SELECT CASE WHEN selected.analysis_id IS NOT NULL THEN (
        SELECT ROW(a.analysis_id, a.job_id, a.source)::public.analysis_choice
        FROM public.analysis_catalog_sources a
        WHERE a.match_id = m.id AND a.analysis_id = selected.analysis_id
          AND selected.source_revision = m.source_revision
        ORDER BY CASE WHEN a.source = 'native' AND a.scope = 'match' AND a.complete THEN 0
                      WHEN a.source = 'imported' THEN 1 ELSE 2 END,
                 a.created_at DESC, a.analysis_id DESC
        LIMIT 1
    ) ELSE COALESCE((
        SELECT ROW(a.analysis_id, a.job_id, a.source)::public.analysis_choice
        FROM public.analysis_catalog_sources a
        WHERE a.match_id = m.id AND a.status = 'completed'
          AND a.source = 'native' AND a.scope = 'match' AND a.complete
        ORDER BY a.created_at DESC, a.analysis_id DESC
        LIMIT 1
    ), (
        SELECT ROW(a.analysis_id, a.job_id, a.source)::public.analysis_choice
        FROM public.analysis_catalog_sources a
        WHERE a.match_id = m.id AND a.status = 'completed' AND a.source = 'imported'
        ORDER BY a.created_at DESC, a.analysis_id DESC
        LIMIT 1
    ), (
        SELECT ROW(a.analysis_id, a.job_id, a.source)::public.analysis_choice
        FROM public.analysis_catalog_sources a
        WHERE a.match_id = m.id AND a.status = 'completed'
          AND (a.source = 'imported' OR a.scope <> 'match' OR a.complete OR m.collection_only)
          AND NOT (a.source = 'native' AND a.scope = 'match' AND a.complete)
          AND a.source <> 'imported'
        ORDER BY a.created_at DESC, a.analysis_id DESC
        LIMIT 1
    )) END AS choice
    -- Evaluate the choice once per match rather than once per column.
    OFFSET 0
) chosen
WHERE (chosen.choice).analysis_id IS NOT NULL;

-- Decision search reads one player's decisions in loss order and filters
-- them on the index entries; only the hits of the returned page are read
-- from the table. The row number orders otherwise equal decisions.
DROP INDEX public.decision_index_player_loss;
CREATE INDEX decision_index_player_loss ON public.decision_index
    (player_name, equity_loss DESC, match_id, game_no, decision_no, seq)
    INCLUDE (question_kind, mistake_kind, game_plan, away_own, away_opponent,
             cube_value, cube_owner);
