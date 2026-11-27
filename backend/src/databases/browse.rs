use super::*;

pub(super) const MATCH_PLAY_JOIN: &str = " LEFT JOIN play_sessions ps ON ps.library_match_id=m.id";

pub(super) const MATCH_NOTE_JOIN: &str = " LEFT JOIN annotations a ON a.match_uuid=m.id";

// Select the PR source independently of any active whole-match job.
// Incomplete or imported selected results never borrow another source's rating.
pub(super) const MATCH_ANALYSIS_JOINS: &str = concat!(
    "
LEFT JOIN LATERAL (
    SELECT a.source,a.job_id FROM effective_analysis_selections a WHERE a.match_id=m.id LIMIT 1
) selected ON true
LEFT JOIN jobs selected_job ON selected_job.id=selected.job_id
LEFT JOIN LATERAL (
    SELECT selected_job.result,selected_job.total_games,selected_job.completed_games
    WHERE selected_job.match_uuid=m.id AND selected_job.source_revision=m.source_revision AND selected_job.kind='match' AND selected_job.game_no IS NULL AND selected_job.status='completed' AND NOT m.collection_only
) rawpr ON true
LEFT JOIN LATERAL (
    SELECT rawpr.result WHERE ",
    crate::match_analysis::rated_match_pr_where_sql!(),
    "
) prj ON true
LEFT JOIN (
    SELECT j.match_uuid,j.source_revision,
           CASE WHEN bool_or(j.status='running') THEN 'running' ELSE 'queued' END AS status
    FROM jobs j
    WHERE j.kind='match' AND j.game_no IS NULL AND j.status IN ('queued','running')
    GROUP BY j.match_uuid,j.source_revision
) active_match_job ON active_match_job.match_uuid=m.id AND active_match_job.source_revision=m.source_revision
    AND NOT m.collection_only
    AND (ps.id IS NULL OR NOT ",
    crate::play::active_competition_sql!("ps"),
    ")
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
) analysis ON true"
);

pub(super) const MATCH_COLUMNS: &str = "coalesce((SELECT r.id FROM study_report_snapshots r WHERE r.session_id=ps.id ORDER BY r.game_no DESC,r.created_at DESC LIMIT 1),CASE WHEN ps.state->>'phase'='match_over' AND jsonb_typeof(ps.state->'review')='object' THEN ps.id END) AS study_report_id,ps.id AS play_session_id,ps.paused_from_database_id IS NOT NULL AS play_paused,ps.state->>'phase' AS play_phase,m.play_mode,m.play_assisted,m.id,NOT m.collection_only AS source_available,coalesce(m.source_match_no,m.match_no) AS match_no,m.filename,m.format,m.white,m.black,m.match_length,m.position_count,m.created_at,coalesce(m.source_database_id,m.database_id) AS database_id,coalesce(nullif(m.display_name,''),m.filename) AS display_name,m.played_on,m.tournament,m.round,m.place,m.result_state,m.white_score,m.black_score,(prj.result #>> '{performance_rating,players,0,value}')::float8 AS white_pr,(prj.result #>> '{performance_rating,players,1,value}')::float8 AS black_pr,prj.result #>> '{performance_rating,method}' AS pr_method,prj.result #>> '{performance_rating,profile}' AS pr_profile,analysis.status AS analysis_status,(SELECT count(*) FROM match_games g WHERE g.match_uuid=m.id) AS game_count,jsonb_array_length(m.import_warnings) AS import_warning_count,coalesce(a.tags,'{}'::text[]) AS tags,coalesce(a.comment,'') AS comment,m.metadata_version AS version";

pub(crate) const DATE_FIELDS: &[&str] = &["played", "imported"];
pub(crate) const MATCH_TYPES: &[&str] = &["match", "money"];
pub(crate) const RESULT_STATES: &[&str] = &[
    "completed",
    "ongoing",
    "unknown",
    "white_won",
    "black_won",
    "draw",
];

pub(crate) fn valid_match_points_range(min: Option<i32>, max: Option<i32>) -> bool {
    min.is_none_or(|v| v >= 0)
        && max.is_none_or(|v| v >= 0)
        && min.zip(max).is_none_or(|(a, b)| a <= b)
}

pub(crate) fn sort_spec(sort: &str) -> Option<(&'static str, &'static str)> {
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
        q.push(" AND m.id IN (SELECT id FROM matches WHERE search_text LIKE ")
            .push_bind(pattern.clone())
            .push(" OR lower(display_name) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR lower(tournament) LIKE ")
            .push_bind(pattern.clone())
            .push(" OR lower(round) LIKE ")
            .push_bind(pattern.clone())
            .push(" UNION SELECT ")
            .push(crate::annotations::ANNOTATION_MATCH_UUID_EXPR)
            .push(" FROM ")
            .push(crate::annotations::ANNOTATION_MATCH_JOIN_SQL)
            .push(" WHERE a.search_text LIKE ")
            .push_bind(pattern)
            .push(")");
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
        q.push(" AND EXISTS(SELECT 1 ")
            .push(crate::annotations::MATCH_ANNOTATION_EXISTS_FROM)
            .push(" AND n.tags @> ")
            .push_bind(vec![tag])
            .push(")");
    }
    if let Some(v) = p.comment.as_ref() {
        q.push(" AND EXISTS(SELECT 1 ")
            .push(crate::annotations::MATCH_ANNOTATION_EXISTS_FROM)
            .push(" AND lower(n.comment) LIKE ")
            .push_bind(literal_pattern(v))
            .push(")");
    }
    if let Some(v) = &p.position_id {
        q.push(concat!(
            " AND (ps.id IS NULL OR NOT ",
            crate::play::active_competition_sql!("ps"),
            ") AND hit.position_id="
        ))
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

struct BrowsePlan {
    sort: String,
    expr: &'static str,
    cast: &'static str,
    direction: String,
    date_field: String,
    limit: i64,
    offset: Option<i64>,
    scope: String,
    occurrences: bool,
    compact: bool,
    sort_uses_analysis: bool,
}

impl BrowsePlan {
    fn occurrence_join(&self) -> &'static str {
        if self.occurrences {
            " JOIN positions hit ON hit.match_uuid=m.id JOIN databases occurrence_db ON occurrence_db.id=m.database_id"
        } else {
            ""
        }
    }

    fn occurrence_columns(&self) -> &'static str {
        if self.occurrences {
            ",hit.id AS occurrence_id,hit.ordinal AS occurrence_ordinal,hit.game_no AS occurrence_game_no,hit.ply AS occurrence_move_no,occurrence_db.name AS occurrence_database_name"
        } else {
            ""
        }
    }

    fn projected_occurrence_columns(&self) -> &'static str {
        if self.occurrences {
            ",ranked.occurrence_id,ranked.occurrence_ordinal,ranked.occurrence_game_no,ranked.occurrence_move_no,ranked.occurrence_database_name"
        } else {
            ""
        }
    }

    fn sort_order(&self) -> &'static str {
        if self.direction == "asc" {
            " ASC NULLS LAST"
        } else {
            " DESC NULLS LAST"
        }
    }
}

fn prepare_browse_plan(p: &mut LibraryQuery) -> ApiResult<(BrowsePlan, Option<String>)> {
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
        bounded_text(v)?;
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
    if !DATE_FIELDS.contains(&date_field.as_str()) {
        return Err(ApiError::invalid("date_field must be played or imported"));
    }
    if p.date_from.zip(p.date_to).is_some_and(|(a, b)| a > b) {
        return Err(ApiError::invalid("date_from must not exceed date_to"));
    }
    if !valid_match_points_range(p.match_points_min, p.match_points_max) {
        return Err(ApiError::invalid("Invalid match points range"));
    }
    if p.match_type
        .as_deref()
        .is_some_and(|v| !MATCH_TYPES.contains(&v))
    {
        return Err(ApiError::invalid("match_type must be match or money"));
    }
    if p.result
        .as_deref()
        .is_some_and(|v| !RESULT_STATES.contains(&v))
    {
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
        Sha256::digest(serde_json::to_vec(p).map_err(anyhow::Error::from)?)
    );
    let occurrences = p.position_id.is_some();
    let compact = ["tags", "comment"].contains(&sort.as_str());
    let sort_uses_analysis = ["white_pr", "black_pr", "analysis_status"].contains(&sort.as_str());
    Ok((
        BrowsePlan {
            sort,
            expr,
            cast,
            direction,
            date_field,
            limit,
            offset,
            scope,
            occurrences,
            compact,
            sort_uses_analysis,
        },
        after,
    ))
}

async fn resolve_browse_cursor(
    pool: &PgPool,
    after: Option<String>,
    plan: &BrowsePlan,
    p: &LibraryQuery,
) -> ApiResult<Option<LibraryCursor>> {
    let Some(raw) = after else {
        return Ok(None);
    };
    if raw.len() > crate::cursor::DEFAULT_MAX_LEN {
        return Err(ApiError::invalid("Cursor is too long"));
    }
    let mut c: LibraryCursor = crate::cursor::decode(
        &raw,
        crate::cursor::DEFAULT_MAX_LEN,
        "Cursor is invalid or belongs to different filters",
    )?;
    if c.scope != plan.scope {
        return Err(ApiError::invalid(
            "Cursor is invalid or belongs to different filters",
        ));
    }
    if c.anchor != plan.compact
        || (c.anchor && c.key.is_some())
        || c.occurrence.is_some() != plan.occurrences
        || c.occurrence.as_ref().is_some_and(|o| o.ordinal < 0)
    {
        return Err(ApiError::invalid("Cursor contains an invalid anchor"));
    }
    if c.anchor {
        // Use the same filters as the page, so a deleted anchor or one that
        // has left the current scope produces a validation error.
        let play_join = if plan.occurrences {
            MATCH_PLAY_JOIN
        } else {
            ""
        };
        let mut anchor = QueryBuilder::<Postgres>::new(format!(
            "SELECT ({})::text AS cursor_key FROM matches m{MATCH_NOTE_JOIN}{play_join}{} WHERE NOT m.collection_only",
            plan.expr,
            plan.occurrence_join()
        ));
        library_filters(&mut anchor, p, &plan.date_field)?;
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
        .contains(&plan.sort.as_str()),
        Some(key) => match plan.cast {
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
    Ok(Some(c))
}

fn push_browse_query<'a>(
    q: &mut QueryBuilder<'a, Postgres>,
    plan: &'a BrowsePlan,
    p: &'a LibraryQuery,
    cursor: Option<LibraryCursor>,
) -> ApiResult<()> {
    let occurrence_join = plan.occurrence_join();
    if plan.offset.is_some() {
        q.push("totals AS (SELECT count(*) AS total FROM matches m");
        if plan.occurrences {
            q.push(MATCH_PLAY_JOIN).push(occurrence_join);
        }
        q.push(" WHERE NOT m.collection_only");
        library_filters(q, p, &plan.date_field)?;
        q.push("), ");
    }
    if plan.sort_uses_analysis {
        q.push("page AS (");
        library_row_projection(q, p.player_scope());
    } else {
        // Sort and limit narrow rows before fetching notes, games and analysis.
        q.push("ranked AS MATERIALIZED (SELECT m.id");
    }
    q.push(plan.occurrence_columns()).push(format!(
        ",({})::text AS cursor_key,({}) AS page_sort FROM matches m",
        plan.expr, plan.expr
    ));
    if plan.occurrences || plan.sort_uses_analysis {
        q.push(MATCH_PLAY_JOIN);
    }
    if plan.compact || plan.sort_uses_analysis {
        q.push(MATCH_NOTE_JOIN);
    }
    if plan.sort_uses_analysis {
        q.push(MATCH_ANALYSIS_JOINS);
    }
    q.push(occurrence_join).push(" WHERE NOT m.collection_only");
    library_filters(q, p, &plan.date_field)?;
    if let Some(c) = cursor {
        q.push(" AND (");
        if let Some(key) = c.key {
            q.push(plan.expr)
                .push(if plan.direction == "asc" {
                    " > "
                } else {
                    " < "
                })
                .push_bind(key.clone())
                .push("::")
                .push(plan.cast)
                .push(" OR ")
                .push(plan.expr)
                .push(" IS NULL OR (")
                .push(plan.expr)
                .push(" = ")
                .push_bind(key)
                .push("::")
                .push(plan.cast)
                .push(" AND ");
            occurrence_after(q, c.id, c.occurrence.as_ref());
            q.push("))");
        } else {
            q.push(plan.expr).push(" IS NULL AND ");
            occurrence_after(q, c.id, c.occurrence.as_ref());
            q.push(")");
        }
    }
    let sort_order = plan.sort_order();
    q.push(" ORDER BY ")
        .push(plan.expr)
        .push(sort_order)
        .push(",m.id ASC");
    if plan.occurrences {
        q.push(",hit.ordinal ASC,hit.id ASC");
    }
    q.push(" LIMIT ").push_bind(plan.limit + 1);
    if let Some(offset) = plan.offset {
        q.push(" OFFSET ").push_bind(offset);
    }
    q.push(")");
    if !plan.sort_uses_analysis {
        q.push(", page AS (");
        library_row_projection(q, p.player_scope());
        q.push(",ranked.cursor_key,ranked.page_sort")
            .push(plan.projected_occurrence_columns())
            .push(" FROM ranked JOIN matches m ON m.id=ranked.id")
            .push(MATCH_PLAY_JOIN)
            .push(MATCH_NOTE_JOIN)
            .push(MATCH_ANALYSIS_JOINS)
            .push(")");
    }
    if plan.offset.is_some() {
        q.push(" SELECT page.*,totals.total FROM totals LEFT JOIN page ON true");
    } else {
        q.push(" SELECT page.* FROM page");
    }
    q.push(" ORDER BY page_sort")
        .push(sort_order)
        .push(",id ASC");
    if plan.occurrences {
        q.push(",occurrence_ordinal ASC,occurrence_id ASC");
    }
    Ok(())
}

fn map_browse_rows(
    mut rows: Vec<sqlx::postgres::PgRow>,
    plan: &BrowsePlan,
    p: &LibraryQuery,
) -> ApiResult<LibraryPage> {
    let total = if plan.offset.is_some() {
        let total = rows[0].get::<i64, _>("total");
        rows.retain(|row| row.get::<Option<Uuid>, _>("id").is_some());
        Some(total)
    } else {
        None
    };
    let more = rows.len() > plan.limit as usize;
    let mut items = Vec::new();
    let mut last = None;
    for row in rows.iter().take(plan.limit as usize) {
        let mut m = <DatabaseMatch as sqlx::FromRow<sqlx::postgres::PgRow>>::from_row(row)
            .map_err(anyhow::Error::from)?;
        if p.player_scope().is_some() {
            m.player_sides = Some(row.try_get("player_sides").map_err(anyhow::Error::from)?);
            m.opponent_player_id = row
                .try_get("opponent_player_id")
                .map_err(anyhow::Error::from)?;
        }
        let occurrence = if plan.occurrences {
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
            scope: plan.scope.clone(),
            key: if plan.compact {
                None
            } else {
                row.try_get("cursor_key").map_err(anyhow::Error::from)?
            },
            id: m.id,
            anchor: plan.compact,
        });
        items.push(m);
    }
    let next_cursor = if more {
        last.map(|c| crate::cursor::encode(&c)).transpose()?
    } else {
        None
    };
    Ok(LibraryPage {
        items,
        next_cursor,
        total,
    })
}

pub async fn browse(pool: &PgPool, mut p: LibraryQuery) -> ApiResult<LibraryPage> {
    let (plan, after) = prepare_browse_plan(&mut p)?;
    let cursor = resolve_browse_cursor(pool, after, &plan, &p).await?;
    let mut q = QueryBuilder::<Postgres>::new("WITH ");
    push_browse_query(&mut q, &plan, &p, cursor)?;
    let rows = q
        .build()
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::from)?;
    map_browse_rows(rows, &plan, &p)
}
