-- Effective selection needs evidence that one usable import exists. The
-- catalog still reads the primary rows when a caller requests counts or depth.
CREATE INDEX imported_analyses_reviewable_match ON public.imported_analyses (match_uuid)
    WHERE kind IN ('checker', 'cube') AND public.imported_analysis_has_values(data);

-- A mixed library looks up a sparse set of explicit selections once per
-- match. Both the key and its validity revision fit in an index-only read.
CREATE INDEX analysis_selections_lookup ON public.analysis_selections (match_id)
    INCLUDE (analysis_id, source_revision);

-- Keep the priority and stale-selection semantics of 0093.
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
          -- The stored key identifies its catalog branch. An imported key
          -- never needs to probe the native jobs of this match's positions.
          AND a.source = CASE WHEN selected.analysis_id LIKE 'imported:%' THEN 'imported' ELSE 'native' END
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
