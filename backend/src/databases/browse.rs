use super::*;

pub(super) const MATCH_PLAY_JOIN: &str = " LEFT JOIN play_sessions ps ON ps.library_match_id=m.id";

pub(super) const MATCH_NOTE_JOIN: &str = " LEFT JOIN annotations a ON a.match_uuid=m.id";

// Select the PR source independently of any active whole-match job.
// Incomplete or imported selected results never borrow another source's rating.
pub(super) const MATCH_ANALYSIS_JOINS: &str = "
LEFT JOIN LATERAL (
    SELECT a.source,a.job_id FROM effective_analysis_selections a WHERE a.match_id=m.id LIMIT 1
) selected ON true
LEFT JOIN jobs selected_job ON selected_job.id=selected.job_id
LEFT JOIN LATERAL (
    SELECT selected_job.result,selected_job.total_games,selected_job.completed_games
    WHERE selected_job.match_uuid=m.id AND selected_job.source_revision=m.source_revision AND selected_job.kind='match' AND selected_job.game_no IS NULL AND selected_job.status='completed' AND NOT m.collection_only
) rawpr ON true
LEFT JOIN LATERAL (
    SELECT rawpr.result WHERE rawpr.total_games>0 AND rawpr.completed_games=rawpr.total_games
    AND NOT m.play_assisted AND (m.play_mode IS NULL OR (m.play_mode='competition' AND (ps.id IS NULL OR (ps.state->>'phase'='match_over' OR (m.match_length=0 AND ps.state->>'phase'='game_over')))))
    AND rawpr.result->>'total_games'=rawpr.total_games::text
    AND rawpr.result->>'completed_games'=rawpr.total_games::text
    AND rawpr.result->>'analyzed_moves'=rawpr.result->>'total_moves'
    AND rawpr.result #>> '{players,0,unavailable}'='0'
    AND rawpr.result #>> '{players,1,unavailable}'='0'
    AND rawpr.result #>> '{performance_rating,method}'='gnubg_emg_500_v1'
    AND rawpr.result #>> '{performance_rating,games}'=rawpr.total_games::text
    AND rawpr.result #>> '{performance_rating,profile}'=(rawpr.result->>'engine')||'; '||(rawpr.result->>'plies')||'-ply cubeful deterministic'
    AND rawpr.result #>> '{performance_rating,players,0,player}'='0'
    AND rawpr.result #>> '{performance_rating,players,1,player}'='1'
) prj ON true
LEFT JOIN (
    SELECT j.match_uuid,j.source_revision,
           CASE WHEN bool_or(j.status='running') THEN 'running' ELSE 'queued' END AS status
    FROM jobs j
    WHERE j.kind='match' AND j.game_no IS NULL AND j.status IN ('queued','running')
    GROUP BY j.match_uuid,j.source_revision
) active_match_job ON active_match_job.match_uuid=m.id AND active_match_job.source_revision=m.source_revision
    AND NOT m.collection_only
    AND (ps.id IS NULL OR coalesce(ps.state->>'play_mode','competition')<>'competition'
         OR ps.state->>'phase'='match_over' OR (m.match_length=0 AND ps.state->>'phase'='game_over'))
LEFT JOIN LATERAL (
    SELECT coalesce(
        active_match_job.status,
        CASE WHEN selected.source='imported' THEN 'imported'
             WHEN selected_job.status='completed' AND (selected_job.kind='analysis' OR selected_job.game_no IS NOT NULL) THEN 'partial'
             ELSE selected_job.status END,
        (SELECT CASE WHEN c.source='imported' THEN 'imported'
                     WHEN c.scope<>'match' AND c.status='completed' THEN 'partial'
                     ELSE c.status END
         FROM analysis_catalog_sources c WHERE c.match_id=m.id
         ORDER BY (c.status='completed') DESC,c.created_at DESC,c.analysis_id DESC LIMIT 1)
    ) AS status
) analysis ON true";

pub(super) const MATCH_COLUMNS: &str = "coalesce((SELECT r.id FROM study_report_snapshots r WHERE r.session_id=ps.id ORDER BY r.game_no DESC,r.created_at DESC LIMIT 1),CASE WHEN ps.state->>'phase'='match_over' AND jsonb_typeof(ps.state->'review')='object' THEN ps.id END) AS study_report_id,ps.id AS play_session_id,ps.paused_from_database_id IS NOT NULL AS play_paused,ps.state->>'phase' AS play_phase,m.play_mode,m.play_assisted,m.id,NOT m.collection_only AS source_available,coalesce(m.source_match_no,m.match_no) AS match_no,m.filename,m.format,m.white,m.black,m.match_length,m.position_count,m.created_at,coalesce(m.source_database_id,m.database_id) AS database_id,coalesce(nullif(m.display_name,''),m.filename) AS display_name,m.played_on,m.tournament,m.round,m.place,m.result_state,m.white_score,m.black_score,(prj.result #>> '{performance_rating,players,0,value}')::float8 AS white_pr,(prj.result #>> '{performance_rating,players,1,value}')::float8 AS black_pr,prj.result #>> '{performance_rating,method}' AS pr_method,prj.result #>> '{performance_rating,profile}' AS pr_profile,analysis.status AS analysis_status,(SELECT count(*) FROM match_games g WHERE g.match_uuid=m.id) AS game_count,jsonb_array_length(m.import_warnings) AS import_warning_count,coalesce(a.tags,'{}'::text[]) AS tags,coalesce(a.comment,'') AS comment,m.metadata_version AS version";

pub(super) fn sort_spec(sort: &str) -> Option<(&'static str, &'static str)> {
    Some(match sort {
        "occurrence_database" => ("lower(occurrence_db.name)", "text"),
        "occurrence_game" => ("hit.game_no", "integer"),
        "occurrence_move" => ("hit.ply", "integer"),
        "id" | "match_no" => ("m.match_no", "bigint"),
        "created_at" => ("m.created_at", "timestamptz"),
        "played_at" => ("m.played_on", "date"),
        "name" => (
            "lower(coalesce(nullif(m.display_name,''),m.filename))",
            "text",
        ),
        "white" => ("lower(m.white)", "text"),
        "black" => ("lower(m.black)", "text"),
        "match_length" => ("m.match_length", "integer"),
        "tournament" => ("lower(nullif(m.tournament,''))", "text"),
        "format" => ("lower(m.format)", "text"),
        "position_count" => ("m.position_count", "integer"),
        "analysis_status" => ("analysis.status", "text"),
        "tags" => ("lower(nullif(array_to_string(a.tags,', '),''))", "text"),
        "comment" => ("lower(nullif(a.comment,''))", "text"),
        "score" => (
            "CASE WHEN m.white_score IS NOT NULL AND m.black_score IS NOT NULL THEN ARRAY[m.white_score,m.black_score] END",
            "integer[]",
        ),
        "result" => ("m.result_state", "text"),
        "white_pr" => (
            "(prj.result #>> '{performance_rating,players,0,value}')::float8",
            "float8",
        ),
        "black_pr" => (
            "(prj.result #>> '{performance_rating,players,1,value}')::float8",
            "float8",
        ),
        _ => return None,
    })
}

pub(super) fn bounded_text(value: &Option<String>) -> ApiResult<()> {
    if value.as_ref().is_some_and(|v| v.chars().count() > 240) {
        return Err(ApiError::invalid(
            "Filter text is limited to 240 characters",
        ));
    }
    Ok(())
}

pub(super) fn valid_score_key(key: &str) -> bool {
    let Some(values) = key.strip_prefix('{').and_then(|v| v.strip_suffix('}')) else {
        return false;
    };
    let values: Vec<_> = values.split(',').collect();
    values.len() == 2
        && values
            .iter()
            .all(|v| v.parse::<i32>().is_ok_and(|n| n >= 0))
}

pub(super) fn like(q: &mut QueryBuilder<'_, Postgres>, expr: &str, value: &Option<String>) {
    if let Some(v) = value.as_deref().filter(|v| !v.trim().is_empty()) {
        q.push(" AND ")
            .push(expr)
            .push(" LIKE ")
            .push_bind(literal_pattern(v.trim()));
    }
}

pub(super) fn library_filters(
    q: &mut QueryBuilder<'_, Postgres>,
    p: &LibraryQuery,
    date_field: &str,
) -> ApiResult<()> {
    if let Some(id) = p.database_id {
        q.push(" AND m.database_id=").push_bind(id);
    }
    if let Some(v) = p.q.as_deref().filter(|v| !v.trim().is_empty()) {
        let pattern = literal_pattern(v.trim());
        q.push(" AND m.id IN (SELECT id FROM matches WHERE search_text LIKE ").push_bind(pattern.clone()).push(" OR lower(display_name) LIKE ").push_bind(pattern.clone()).push(" OR lower(tournament) LIKE ").push_bind(pattern.clone()).push(" OR lower(round) LIKE ").push_bind(pattern.clone()).push(" UNION SELECT coalesce(n.match_uuid,np.match_uuid,ng.match_uuid,nm.match_uuid) FROM annotations n LEFT JOIN positions np ON np.id=n.position_uuid LEFT JOIN match_games ng ON ng.id=n.game_uuid LEFT JOIN match_moves nm ON nm.id=n.move_uuid WHERE n.search_text LIKE ").push_bind(pattern).push(")");
    }
    if let Some((id, virtual_profile)) = p.player_scope() {
        q.push(" AND EXISTS(SELECT 1 ")
            .push(crate::player_scope::match_sides_from(virtual_profile))
            .push_bind(id)
            .push(")");
    }
    if let Some(v) = p.player.as_deref().filter(|v| !v.trim().is_empty()) {
        let pattern = literal_pattern(v.trim());
        q.push(" AND (lower(m.white) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR lower(m.black) LIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if date_field == "played" {
        if let Some(v) = p.date_from {
            q.push(" AND m.played_on >= ").push_bind(v);
        }
        if let Some(v) = p.date_to {
            q.push(" AND m.played_on <= ").push_bind(v);
        }
    } else {
        if let Some(v) = p.date_from {
            q.push(" AND m.created_at >= ")
                .push_bind(v.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }
        if let Some(v) = p.date_to {
            let end = v
                .succ_opt()
                .ok_or_else(|| ApiError::invalid("date_to is out of range"))?;
            q.push(" AND m.created_at < ")
                .push_bind(end.and_hms_opt(0, 0, 0).unwrap().and_utc());
        }
    }
    like(q, "lower(m.tournament)", &p.tournament);
    like(q, "lower(m.round)", &p.round);
    like(
        q,
        "lower(coalesce(nullif(m.display_name,''),m.filename))",
        &p.match_name,
    );
    if let Some(v) = p.tag.as_ref() {
        let tag = normalize_tag(v)?;
        q.push(" AND EXISTS(SELECT 1 FROM annotations n LEFT JOIN positions np ON np.id=n.position_uuid LEFT JOIN match_games ng ON ng.id=n.game_uuid LEFT JOIN match_moves nm ON nm.id=n.move_uuid WHERE coalesce(n.match_uuid,np.match_uuid,ng.match_uuid,nm.match_uuid)=m.id AND n.tags @> ").push_bind(vec![tag]).push(")");
    }
    if let Some(v) = p.comment.as_ref() {
        q.push(" AND EXISTS(SELECT 1 FROM annotations n LEFT JOIN positions np ON np.id=n.position_uuid LEFT JOIN match_games ng ON ng.id=n.game_uuid LEFT JOIN match_moves nm ON nm.id=n.move_uuid WHERE coalesce(n.match_uuid,np.match_uuid,ng.match_uuid,nm.match_uuid)=m.id AND lower(n.comment) LIKE ").push_bind(literal_pattern(v)).push(")");
    }
    if let Some(v) = &p.position_id {
        q.push(" AND (ps.id IS NULL OR coalesce(ps.state->>'play_mode','competition')<>'competition' OR ps.state->>'phase'='match_over' OR (m.match_length=0 AND ps.state->>'phase'='game_over')) AND hit.position_id=")
            .push_bind(v.clone());
    }
    if let Some(v) = &p.match_type {
        q.push(if v == "money" {
            " AND m.match_length=0"
        } else {
            " AND m.match_length>0"
        });
    }
    if let Some(v) = &p.result {
        match v.as_str() {
            "white_won" | "black_won" | "draw" => {
                q.push(" AND m.result_state='completed' AND m.white_score IS NOT NULL AND m.black_score IS NOT NULL AND m.white_score")
                    .push(match v.as_str() {"white_won"=>" > ","black_won"=>" < ",_=>" = "})
                    .push("m.black_score");
            }
            _ => {
                q.push(" AND m.result_state=").push_bind(v.clone());
            }
        }
    }
    if let Some(v) = p.match_points_min {
        q.push(" AND m.match_length>=").push_bind(v);
    }
    if let Some(v) = p.match_points_max {
        q.push(" AND m.match_length<=").push_bind(v);
    }
    Ok(())
}

pub(super) fn occurrence_after(
    q: &mut QueryBuilder<'_, Postgres>,
    id: Uuid,
    occurrence: Option<&OccurrenceCursor>,
) {
    if let Some(o) = occurrence {
        q.push("(m.id,hit.ordinal,hit.id) > (")
            .push_bind(id)
            .push(",")
            .push_bind(o.ordinal)
            .push(",")
            .push_bind(o.id)
            .push(")");
    } else {
        q.push("m.id > ").push_bind(id);
    }
}

pub(super) fn library_row_projection(
    q: &mut QueryBuilder<'_, Postgres>,
    player: Option<(Uuid, bool)>,
) {
    q.push("SELECT ").push(MATCH_COLUMNS);
    if let Some((id, virtual_profile)) = player {
        let sides = crate::player_scope::match_sides_from(virtual_profile);
        q.push(",ARRAY(SELECT DISTINCT mp.side ")
            .push(sides)
            .push_bind(id)
            .push(" ORDER BY mp.side) AS player_sides")
            // The other side's profile; a match against oneself has none.
            .push(",(SELECT a.profile_id FROM match_players o JOIN player_aliases a ON a.id=o.alias_id WHERE o.match_id=m.id AND o.side NOT IN (SELECT mp.side ")
            .push(sides)
            .push_bind(id)
            .push(") ORDER BY o.side LIMIT 1) AS opponent_player_id");
    }
}

pub async fn browse(pool: &PgPool, mut p: LibraryQuery) -> ApiResult<LibraryPage> {
    for v in [
        &p.q,
        &p.player,
        &p.tag,
        &p.comment,
        &p.position_id,
        &p.tournament,
        &p.round,
        &p.match_name,
    ] {
        bounded_text(v)?
    }
    if p.player_id.is_some() && p.virtual_player_id.is_some() {
        return Err(ApiError::invalid(
            "Choose either player_id or virtual_player_id",
        ));
    }
    let sort = p.sort.clone().unwrap_or("created_at".into());
    if p.position_id.is_none()
        && ["occurrence_database", "occurrence_game", "occurrence_move"].contains(&sort.as_str())
    {
        return Err(ApiError::invalid(
            "Occurrence sorting requires a position filter",
        ));
    }
    let (expr, cast) = sort_spec(&sort).ok_or_else(|| ApiError::invalid("Unknown sort field"))?;
    let direction = p.direction.clone().unwrap_or("desc".into());
    if !["asc", "desc"].contains(&direction.as_str()) {
        return Err(ApiError::invalid("direction must be asc or desc"));
    }
    let date_field = p.date_field.clone().unwrap_or("played".into());
    if !["played", "imported"].contains(&date_field.as_str()) {
        return Err(ApiError::invalid("date_field must be played or imported"));
    }
    if p.date_from.zip(p.date_to).is_some_and(|(a, b)| a > b) {
        return Err(ApiError::invalid("date_from must not exceed date_to"));
    }
    if p.match_points_min.is_some_and(|v| v < 0)
        || p.match_points_max.is_some_and(|v| v < 0)
        || p.match_points_min
            .zip(p.match_points_max)
            .is_some_and(|(a, b)| a > b)
    {
        return Err(ApiError::invalid("Invalid match points range"));
    }
    if p.match_type
        .as_deref()
        .is_some_and(|v| !["match", "money"].contains(&v))
    {
        return Err(ApiError::invalid("match_type must be match or money"));
    }
    if p.result.as_deref().is_some_and(|v| {
        ![
            "completed",
            "ongoing",
            "unknown",
            "white_won",
            "black_won",
            "draw",
        ]
        .contains(&v)
    }) {
        return Err(ApiError::invalid("Invalid result state"));
    }
    let limit = p.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(ApiError::invalid("limit must be between 1 and 200"));
    }
    let after = p.after.take();
    let offset = crate::search::page_offset(p.page.take(), after.is_some(), limit)?;
    p.limit = None;
    let scope = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&p).map_err(anyhow::Error::from)?)
    );
    let occurrences = p.position_id.is_some();
    let occurrence_join = if occurrences {
        " JOIN positions hit ON hit.match_uuid=m.id JOIN databases occurrence_db ON occurrence_db.id=m.database_id"
    } else {
        ""
    };
    let occurrence_columns = if occurrences {
        ",hit.id AS occurrence_id,hit.ordinal AS occurrence_ordinal,hit.game_no AS occurrence_game_no,hit.ply AS occurrence_move_no,occurrence_db.name AS occurrence_database_name"
    } else {
        ""
    };
    let projected_occurrence_columns = if occurrences {
        ",ranked.occurrence_id,ranked.occurrence_ordinal,ranked.occurrence_game_no,ranked.occurrence_move_no,ranked.occurrence_database_name"
    } else {
        ""
    };
    let compact = ["tags", "comment"].contains(&sort.as_str());
    let sort_uses_analysis = ["white_pr", "black_pr", "analysis_status"].contains(&sort.as_str());
    let mut cursor = after
        .map(|v| {
            if v.len() > 4096 {
                return Err(ApiError::invalid("Cursor is too long"));
            }
            URL_SAFE_NO_PAD
                .decode(v)
                .ok()
                .and_then(|b| serde_json::from_slice::<LibraryCursor>(&b).ok())
                .filter(|c| c.scope == scope)
                .ok_or_else(|| {
                    ApiError::invalid("Cursor is invalid or belongs to different filters")
                })
        })
        .transpose()?;
    if let Some(c) = &mut cursor {
        if c.anchor != compact
            || (c.anchor && c.key.is_some())
            || c.occurrence.is_some() != occurrences
            || c.occurrence.as_ref().is_some_and(|o| o.ordinal < 0)
        {
            return Err(ApiError::invalid("Cursor contains an invalid anchor"));
        }
        if c.anchor {
            // Use the same filters as the page, so a deleted anchor or one that
            // has left the current scope produces a validation error.
            let play_join = if occurrences { MATCH_PLAY_JOIN } else { "" };
            let mut anchor = QueryBuilder::<Postgres>::new(format!(
                "SELECT ({expr})::text AS cursor_key FROM matches m{MATCH_NOTE_JOIN}{play_join}{occurrence_join} WHERE NOT m.collection_only"
            ));
            library_filters(&mut anchor, &p, &date_field)?;
            anchor.push(" AND m.id=").push_bind(c.id);
            if let Some(o) = &c.occurrence {
                anchor
                    .push(" AND hit.id=")
                    .push_bind(o.id)
                    .push(" AND hit.ordinal=")
                    .push_bind(o.ordinal);
            }
            let row = anchor
                .build()
                .fetch_optional(pool)
                .await
                .map_err(anyhow::Error::from)?
                .ok_or_else(|| {
                    ApiError::invalid("Cursor anchor is missing from the current filters")
                })?;
            c.key = row.try_get("cursor_key").map_err(anyhow::Error::from)?;
        }
        let valid = match c.key.as_deref() {
            None => [
                "played_at",
                "white_pr",
                "black_pr",
                "tournament",
                "analysis_status",
                "tags",
                "comment",
                "score",
            ]
            .contains(&sort.as_str()),
            Some(key) => match cast {
                "integer" => key.parse::<i32>().is_ok(),
                "bigint" => key.parse::<i64>().is_ok(),
                "integer[]" => valid_score_key(key),
                "float8" => key.parse::<f64>().is_ok_and(f64::is_finite),
                "date" => NaiveDate::parse_from_str(key, "%Y-%m-%d").is_ok(),
                "timestamptz" => DateTime::parse_from_str(key, "%Y-%m-%d %H:%M:%S%.f%#z").is_ok(),
                _ => c.anchor || key.chars().count() <= 2048,
            },
        };
        if !valid {
            return Err(ApiError::invalid("Cursor contains an invalid sort value"));
        }
    }
    let mut q = QueryBuilder::<Postgres>::new("WITH ");
    if offset.is_some() {
        q.push("totals AS (SELECT count(*) AS total FROM matches m");
        if occurrences {
            q.push(MATCH_PLAY_JOIN).push(occurrence_join);
        }
        q.push(" WHERE NOT m.collection_only");
        library_filters(&mut q, &p, &date_field)?;
        q.push("), ");
    }
    if sort_uses_analysis {
        q.push("page AS (");
        library_row_projection(&mut q, p.player_scope());
    } else {
        // Sort and limit narrow rows before fetching notes, games and analysis.
        q.push("ranked AS MATERIALIZED (SELECT m.id");
    }
    q.push(occurrence_columns).push(format!(
        ",({expr})::text AS cursor_key,({expr}) AS page_sort FROM matches m"
    ));
    if occurrences || sort_uses_analysis {
        q.push(MATCH_PLAY_JOIN);
    }
    if compact || sort_uses_analysis {
        q.push(MATCH_NOTE_JOIN);
    }
    if sort_uses_analysis {
        q.push(MATCH_ANALYSIS_JOINS);
    }
    q.push(occurrence_join).push(" WHERE NOT m.collection_only");
    library_filters(&mut q, &p, &date_field)?;
    if let Some(c) = cursor {
        q.push(" AND (");
        if let Some(key) = c.key {
            q.push(expr)
                .push(if direction == "asc" { " > " } else { " < " })
                .push_bind(key.clone())
                .push("::")
                .push(cast)
                .push(" OR ")
                .push(expr)
                .push(" IS NULL OR (")
                .push(expr)
                .push(" = ")
                .push_bind(key)
                .push("::")
                .push(cast)
                .push(" AND ");
            occurrence_after(&mut q, c.id, c.occurrence.as_ref());
            q.push("))");
        } else {
            q.push(expr).push(" IS NULL AND ");
            occurrence_after(&mut q, c.id, c.occurrence.as_ref());
            q.push(")");
        }
    }
    let sort_order = if direction == "asc" {
        " ASC NULLS LAST"
    } else {
        " DESC NULLS LAST"
    };
    q.push(" ORDER BY ")
        .push(expr)
        .push(sort_order)
        .push(",m.id ASC");
    if occurrences {
        q.push(",hit.ordinal ASC,hit.id ASC");
    }
    q.push(" LIMIT ").push_bind(limit + 1);
    if let Some(offset) = offset {
        q.push(" OFFSET ").push_bind(offset);
    }
    q.push(")");
    if !sort_uses_analysis {
        q.push(", page AS (");
        library_row_projection(&mut q, p.player_scope());
        q.push(",ranked.cursor_key,ranked.page_sort")
            .push(projected_occurrence_columns)
            .push(" FROM ranked JOIN matches m ON m.id=ranked.id")
            .push(MATCH_PLAY_JOIN)
            .push(MATCH_NOTE_JOIN)
            .push(MATCH_ANALYSIS_JOINS)
            .push(")");
    }
    if offset.is_some() {
        q.push(" SELECT page.*,totals.total FROM totals LEFT JOIN page ON true");
    } else {
        q.push(" SELECT page.* FROM page");
    }
    q.push(" ORDER BY page_sort")
        .push(sort_order)
        .push(",id ASC");
    if occurrences {
        q.push(",occurrence_ordinal ASC,occurrence_id ASC");
    }
    let mut rows = q
        .build()
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::from)?;
    let total = if offset.is_some() {
        let total = rows[0].get::<i64, _>("total");
        rows.retain(|row| row.get::<Option<Uuid>, _>("id").is_some());
        Some(total)
    } else {
        None
    };
    let more = rows.len() > limit as usize;
    let mut items = Vec::new();
    let mut last = None;
    for row in rows.iter().take(limit as usize) {
        let mut m = <DatabaseMatch as sqlx::FromRow<sqlx::postgres::PgRow>>::from_row(row)
            .map_err(anyhow::Error::from)?;
        if p.player_scope().is_some() {
            m.player_sides = Some(row.try_get("player_sides").map_err(anyhow::Error::from)?);
            m.opponent_player_id = row
                .try_get("opponent_player_id")
                .map_err(anyhow::Error::from)?;
        }
        let occurrence = if occurrences {
            let id = row.try_get("occurrence_id").map_err(anyhow::Error::from)?;
            m.occurrence = Some(LibraryOccurrence {
                id,
                game_no: row
                    .try_get("occurrence_game_no")
                    .map_err(anyhow::Error::from)?,
                move_no: row
                    .try_get("occurrence_move_no")
                    .map_err(anyhow::Error::from)?,
                database_name: row
                    .try_get("occurrence_database_name")
                    .map_err(anyhow::Error::from)?,
            });
            Some(OccurrenceCursor {
                id,
                ordinal: row
                    .try_get("occurrence_ordinal")
                    .map_err(anyhow::Error::from)?,
            })
        } else {
            None
        };
        last = Some(LibraryCursor {
            occurrence,
            scope: scope.clone(),
            key: if compact {
                None
            } else {
                row.try_get("cursor_key").map_err(anyhow::Error::from)?
            },
            id: m.id,
            anchor: compact,
        });
        items.push(m);
    }
    let next_cursor = if more {
        last.map(|c| serde_json::to_vec(&c).map(|v| URL_SAFE_NO_PAD.encode(v)))
            .transpose()
            .map_err(anyhow::Error::from)?
    } else {
        None
    };
    Ok(LibraryPage {
        items,
        next_cursor,
        total,
    })
}
