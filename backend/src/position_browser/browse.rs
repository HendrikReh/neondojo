use super::*;

pub(super) fn text_filter(q: &mut QueryBuilder<'_, Postgres>, column: &str, text: &Option<String>) {
    if let Some(text) = text {
        q.push(" AND lower(")
            .push(column)
            .push(") LIKE ")
            .push_bind(literal_pattern(text))
            .push(" ESCAPE '\\'");
    }
}

pub(super) const NOTE_SCOPE: &str = "(a.match_uuid=p.match_uuid OR a.position_uuid=p.id OR a.game_uuid=g.id OR a.move_uuid IN (SELECT mv.id FROM match_moves mv WHERE mv.position_uuid=p.id AND mv.match_uuid=p.match_uuid AND mv.game_no=p.game_no AND mv.kind='checker'))";

pub(super) const LOSS: &str =
    "coalesce(native.equity_loss,(imported.projection#>>'{decision,equity_loss}')::float8)";

pub(super) const NAME: &str = "lower(coalesce(nullif(m.display_name,''),m.filename))";

// A global-library cursor must never resume inside the explicit saved collection.
pub(super) const CURSOR_SCOPE: &str = "saved-positions-v1:";

// Matches learning::card_signature; source occurrence UUIDs can differ for one shared card.
pub(super) const CHECKER_SIGNATURE: &str =
    "(p.position_id || ':' || p.gnubg_match_id || ':checker')";

pub(super) fn table_sort_expression(sort: PositionBrowserSort) -> String {
    match sort {
        PositionBrowserSort::Match => NAME.into(),
        PositionBrowserSort::Loss => LOSS.into(),
        PositionBrowserSort::MatchId => "coalesce(m.source_match_no,m.match_no)".into(),
        PositionBrowserSort::Database => "lower(coalesce(m.source_database_name,d.name))".into(),
        PositionBrowserSort::Game => "p.game_no".into(),
        PositionBrowserSort::Ply => "p.ply".into(),
        PositionBrowserSort::Player => "lower(nullif(btrim(CASE WHEN p.player=0 THEN m.white ELSE m.black END),''))".into(),
        PositionBrowserSort::Opponent => "lower(nullif(btrim(CASE WHEN p.player=0 THEN m.black ELSE m.white END),''))".into(),
        PositionBrowserSort::Dice => "CASE WHEN p.dice[1] IS NOT NULL AND p.dice[2] IS NOT NULL THEN ARRAY[least(p.dice[1],p.dice[2]),greatest(p.dice[1],p.dice[2])] END".into(),
        PositionBrowserSort::Score => "CASE WHEN p.source_white_score IS NOT NULL AND p.source_black_score IS NOT NULL THEN ARRAY[p.source_white_score,p.source_black_score] END".into(),
        PositionBrowserSort::Cube => "p.source_cube_value".into(),
        PositionBrowserSort::PlayedMove => "lower(nullif(btrim(p.notation),''))".into(),
        PositionBrowserSort::Decks => format!("(SELECT string_agg(lower(dk.name),chr(10) ORDER BY lower(dk.name) COLLATE \"C\",dk.name COLLATE \"C\",dk.id) FROM learning_cards c JOIN deck_cards dc ON dc.card_id=c.id JOIN decks dk ON dk.id=dc.deck_id WHERE c.signature={CHECKER_SIGNATURE})"),
        PositionBrowserSort::Tags => format!("nullif(ARRAY(SELECT DISTINCT tag FROM annotations a CROSS JOIN LATERAL unnest(a.tags) tag WHERE {NOTE_SCOPE} ORDER BY tag),ARRAY[]::text[])"),
        PositionBrowserSort::Comment => format!("(SELECT string_agg(lower(a.comment),chr(10) ORDER BY a.target_kind,a.id) FROM annotations a WHERE {NOTE_SCOPE} AND btrim(a.comment)<>'')"),
    }
}

pub(super) fn table_order(sql: &mut QueryBuilder<'_, Postgres>, alias: &str, direction: &str) {
    sql.push(" ORDER BY ")
        .push(alias)
        .push(".sort_value ")
        .push(direction)
        .push(" NULLS LAST,")
        .push(alias)
        .push(".sort_name,")
        .push(alias)
        .push(".sort_match_id,")
        .push(alias)
        .push(".sort_ordinal,")
        .push(alias)
        .push(".sort_id");
}

// Read the same chosen catalog entry as card creation. A missing result in the
// selected scope never exposes an older hint or another engine's match result.
pub(super) const READY: &str = "CASE WHEN selected_job.kind='analysis' AND selected_job.position_uuid=p.id THEN jsonb_array_length(selected_job.result->'candidates')>0 AND btrim(coalesce(selected_job.result#>>'{candidates,0,notation}',''))<>'' ELSE native.job_id IS NOT NULL OR imported.projection IS NOT NULL END";

pub(super) const NATIVE_SOLUTION: &str = "btrim(coalesce(chosen.data->>'best',''))<>'' AND ((chosen.data->>'evaluation_status'='forced' AND chosen.equity_loss=0 AND chosen.data->'equity_loss'='0'::jsonb) OR (chosen.data->>'evaluation_status'='evaluated' AND jsonb_typeof(chosen.data->'played_equity')='number' AND jsonb_typeof(chosen.data->'best_equity')='number'))";

pub(super) fn push_native_analysis(sql: &mut QueryBuilder<'_, Postgres>) {
    // OFFSET 0 keeps the selection per match. Joined directly, PostgreSQL
    // may build the analysis catalog of the whole library first.
    sql.push(" LEFT JOIN LATERAL (SELECT effective.analysis_id,effective.job_id,effective.source FROM effective_analysis_selections effective WHERE effective.match_id=m.id OFFSET 0) selected ON true LEFT JOIN jobs selected_job ON selected_job.id=selected.job_id LEFT JOIN LATERAL (SELECT catalog.engine FROM analysis_catalog_sources catalog WHERE catalog.match_id=m.id AND catalog.analysis_id=selected.analysis_id AND selected.source='native' AND nullif(selected_job.result->>'engine','') IS NULL LIMIT 1) fallback_source ON true")
        .push(" LEFT JOIN LATERAL (SELECT chosen.job_id,chosen.engine,chosen.plies,chosen.equity_loss FROM (SELECT selected_job.id AS job_id,coalesce(nullif(selected_job.result->>'engine',''),fallback_source.engine) AS engine,selected_job.plies,ma.equity_loss,ma.data FROM move_analyses ma WHERE ma.job_id=selected_job.id AND ma.position_uuid=p.id AND ma.kind='checker' AND ma.game_no=p.game_no AND selected_job.kind='match' AND (selected_job.game_no IS NULL OR selected_job.game_no=p.game_no) ORDER BY ma.id DESC LIMIT 1) chosen WHERE ")
        .push(NATIVE_SOLUTION)
        .push(") native ON true")
        .push(" LEFT JOIN LATERAL (SELECT cached.projection,ia.id AS imported_id,concat_ws(' ',ia.data->>'engine',ia.data->>'engine_version') AS engine FROM primary_imported_analyses ia JOIN imported_analysis_projections cached ON cached.imported_analysis_id=ia.id WHERE selected.source='imported' AND ia.match_uuid=m.id AND ia.position_uuid=p.id AND ia.kind='checker' AND cached.source_revision=m.source_revision AND cached.revision=")
        .push(crate::imported_analysis::projections::REVISION.to_string())
        .push(" ORDER BY ia.source_ordinal LIMIT 1) imported ON true");
}

pub(super) async fn database_exists(pool: &PgPool, id: Option<Uuid>) -> ApiResult<()> {
    if let Some(id) = id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM databases WHERE id=$1 AND NOT collection_only)",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(anyhow::Error::from)?;
        if !exists {
            return Err(ApiError::invalid(
                "The filter's database no longer exists; choose an existing database explicitly",
            ));
        }
    }
    Ok(())
}

#[utoipa::path(get,path="/api/position-browser",params(PositionBrowserQuery),responses((status=200,body=PositionBrowserPage),(status=422,body=ErrorBody)),tag="Positions")]
pub(super) async fn search(
    State(s): State<AppState>,
    Query(query): Query<PositionBrowserQuery>,
) -> ApiResult<Json<PositionBrowserPage>> {
    let filters = match query.filters {
        Some(value) if value.len() <= 16000 => {
            serde_json::from_str::<PositionFilterDefinition>(&value)
                .map_err(|e| ApiError::invalid(format!("Invalid position filters: {e}")))?
        }
        Some(_) => return Err(ApiError::invalid("Position filter definition is too large")),
        None => PositionFilterDefinition::default(),
    }
    .validate()?;
    let limit = query.limit.unwrap_or(25);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit must be 1–100"));
    }
    let offset = crate::search::page_offset(query.page, query.after.is_some(), limit)?;
    database_exists(&s.pool, filters.database_id).await?;
    if filters.game_plan.is_some() {
        crate::position_game_plans::ensure_saved(&s.pool, filters.database_id).await?;
    }
    let scope = format!(
        "{CURSOR_SCOPE}{:x}",
        Sha256::digest(serde_json::to_vec(&filters).map_err(anyhow::Error::from)?)
    );
    let legacy_order = filters.direction.is_none()
        && matches!(
            filters.sort,
            PositionBrowserSort::Match | PositionBrowserSort::Loss
        );
    let direction = browse_direction(&filters);
    let cursor = if let Some(after) = query.after {
        if after.len() > 4096 {
            return Err(ApiError::invalid("Invalid position cursor"));
        }
        let cursor: BrowserCursor = URL_SAFE_NO_PAD
            .decode(after)
            .ok()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .ok_or_else(|| ApiError::invalid("Invalid position cursor"))?;
        let valid = match &cursor {
            BrowserCursor::Legacy(cursor) => {
                legacy_order
                    && cursor.scope == scope
                    && !cursor.loss.is_some_and(|n| !n.is_finite() || n < 0.0)
            }
            BrowserCursor::Table(cursor) => !legacy_order && cursor.scope == scope,
        };
        if !valid {
            return Err(ApiError::invalid(
                "Cursor does not match the position filters and sort",
            ));
        }
        Some(cursor)
    } else {
        None
    };
    let anchor = match &cursor {
        Some(BrowserCursor::Legacy(cursor)) => Some(cursor.id),
        Some(BrowserCursor::Table(cursor)) => Some(cursor.anchor),
        None => None,
    };
    crate::imported_analysis::projections::ensure_saved(&s.pool, filters.database_id).await?;
    let mut sql = QueryBuilder::<Postgres>::new("");
    push_browse_query(&mut sql, &filters, direction, limit, offset, anchor);
    let mut rows = sql
        .build()
        .fetch_all(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
    if anchor.is_some() {
        if rows
            .first()
            .is_some_and(|row| !row.get::<bool, _>("anchor_exists"))
        {
            return Err(ApiError::invalid(
                "Position cursor anchor no longer matches the filters",
            ));
        }
        rows.retain(|row| {
            row.get::<Option<serde_json::Value>, _>("position")
                .is_some()
        });
    }
    let total = if offset.is_some() {
        let total = rows[0].get::<i64, _>("total");
        rows.retain(|row| row.get::<Option<Uuid>, _>("sort_id").is_some());
        Some(total)
    } else {
        None
    };
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let mut items = Vec::with_capacity(rows.len());
    let mut next_cursor = None;
    let thresholds = crate::configuration::store::error_thresholds(&s.pool).await?;
    for row in rows {
        let position: Position =
            serde_json::from_value(row.get("position")).map_err(anyhow::Error::from)?;
        if more {
            next_cursor = Some(
                URL_SAFE_NO_PAD.encode(
                    serde_json::to_vec(&if legacy_order {
                        BrowserCursor::Legacy(Cursor {
                            scope: scope.clone(),
                            name: row.get("sort_name"),
                            match_id: position.match_uuid,
                            ordinal: position.ordinal,
                            id: position.id,
                            loss: row.get("equity_loss"),
                        })
                    } else {
                        BrowserCursor::Table(TableCursor {
                            scope: scope.clone(),
                            anchor: position.id,
                        })
                    })
                    .map_err(anyhow::Error::from)?,
                ),
            );
        }
        let job_id: Option<Uuid> = row.get("job_id");
        let native_error =
            row.get::<Option<f64>, _>("equity_loss")
                .map(|loss| PositionNativeError {
                    job_id,
                    analysis_id: job_id
                        .map(|id| format!("job:{id}"))
                        .unwrap_or_else(|| format!("imported:{}", position.match_uuid)),
                    imported_analysis_id: row.get("imported_id"),
                    engine: row.get("engine"),
                    plies: row.get("plies"),
                    depth: row.get("depth"),
                    equity_loss: loss,
                    severity: thresholds.classify(loss),
                });
        let context: serde_json::Value = row.get("position");
        items.push(PositionBrowserHit {
            position,
            source_available: row.get("source_available"),
            match_info: serde_json::from_value(row.get("match_info"))
                .map_err(anyhow::Error::from)?,
            match_no: row.get("match_no"),
            database_id: row.get("database_id"),
            database_name: row.get("database_name"),
            display_name: row.get("display_name"),
            tournament: row.get("tournament"),
            round: row.get("round"),
            white_score: context["source_white_score"].as_i64().map(|v| v as i32),
            black_score: context["source_black_score"].as_i64().map(|v| v as i32),
            crawford: context["source_crawford"].as_bool(),
            cube_value: context["source_cube_value"].as_i64().map(|v| v as i32),
            cube_owner: context["source_cube_owner"].as_i64().map(|v| v as i32),
            tags: Vec::new(),
            notes: Vec::new(),
            card_ready: row.get("card_ready"),
            decks: Vec::new(),
            native_error,
        });
    }
    load_notes(&s.pool, &mut items).await?;
    load_deck_memberships(&s.pool, &mut items).await?;
    Ok(Json(PositionBrowserPage {
        items,
        next_cursor,
        total,
    }))
}

/// Omitted: loss descending; all other columns ascending.
pub(super) fn browse_direction(filters: &PositionFilterDefinition) -> &'static str {
    match filters
        .direction
        .unwrap_or(if filters.sort == PositionBrowserSort::Loss {
            PositionBrowserDirection::Desc
        } else {
            PositionBrowserDirection::Asc
        }) {
        PositionBrowserDirection::Asc => "ASC",
        PositionBrowserDirection::Desc => "DESC",
    }
}

/// The page query of the saved-position browser: rows of one filtered
/// snapshot in `direction`, at most `limit + 1`, after `anchor` or from `offset`
/// (with the total), or the first rows when neither is given.
pub(super) fn push_browse_query(
    sql: &mut QueryBuilder<'_, Postgres>,
    filters: &PositionFilterDefinition,
    direction: &'static str,
    limit: i64,
    offset: Option<i64>,
    anchor: Option<Uuid>,
) {
    // Every sort and cursor anchor uses the same saved, filtered snapshot of
    // sort keys only. Analysis is joined for every saved row only when the
    // sort or a filter needs it; the page projection reads it for the at most
    // `limit + 1` page rows.
    let ranks_analysis = filters.sort == PositionBrowserSort::Loss
        || filters.analysis_state.is_some()
        || filters.min_loss.is_some()
        || filters.max_loss.is_some();
    // A numbered page counts and ranks the same rows: materialized, they are
    // read once. An anchor lookup stays inlined to use the primary key.
    sql.push(if anchor.is_none() && offset.is_some() {
        "WITH filtered AS MATERIALIZED (SELECT "
    } else {
        "WITH filtered AS NOT MATERIALIZED (SELECT "
    });
    sql.push(table_sort_expression(filters.sort))
        .push(" AS sort_value,")
        .push(NAME)
        .push(" AS sort_name,m.id AS sort_match_id,p.ordinal AS sort_ordinal,p.id AS sort_id FROM positions p JOIN saved_positions saved ON saved.position_uuid=p.id JOIN matches m ON m.id=p.match_uuid JOIN databases d ON d.id=m.database_id LEFT JOIN match_games g ON g.match_uuid=p.match_uuid AND g.game_no=p.game_no");
    if ranks_analysis {
        push_native_analysis(sql);
    }
    sql.push(" WHERE ").push(SAVED_VISIBLE);
    if let Some(plan) = filters.game_plan {
        sql.push(" AND EXISTS(SELECT 1 FROM position_game_plans plans WHERE plans.position_uuid=p.id AND plans.classifier=")
            .push_bind(crate::game_plan::CLASSIFIER)
            .push(" AND plans.game_plan=")
            .push_bind(crate::position_game_plans::label(plan))
            .push(")");
    }
    if let Some(id) = filters.database_id {
        sql.push(" AND coalesce(m.source_database_id,m.database_id)=")
            .push_bind(id);
    }
    text_filter(
        sql,
        "CASE WHEN p.player=0 THEN m.white ELSE m.black END",
        &filters.player,
    );
    text_filter(
        sql,
        "coalesce(nullif(m.display_name,''),m.filename)",
        &filters.match_name,
    );
    text_filter(sql, "m.tournament", &filters.tournament);
    text_filter(sql, "m.round", &filters.round);
    if let Some(id) = &filters.position_id {
        sql.push(" AND p.position_id=").push_bind(id.clone());
    }
    for (column, value, operator) in [
        ("m.match_length", filters.match_points_min, ">="),
        ("m.match_length", filters.match_points_max, "<="),
        ("p.source_white_score", filters.white_score, "="),
        ("p.source_black_score", filters.black_score, "="),
        ("p.source_cube_value", filters.cube_value, "="),
    ] {
        if let Some(value) = value {
            sql.push(" AND ")
                .push(column)
                .push(operator)
                .push_bind(value);
        }
    }
    if let Some(value) = filters.crawford {
        sql.push(" AND p.source_crawford=").push_bind(value);
    }
    match (filters.die1, filters.die2) {
        (Some(a), Some(b)) => {
            sql.push(" AND least(p.dice[1],p.dice[2])=")
                .push_bind(a)
                .push(" AND greatest(p.dice[1],p.dice[2])=")
                .push_bind(b);
        }
        (Some(a), None) | (None, Some(a)) => {
            sql.push(" AND p.dice @> ").push_bind(vec![a]);
        }
        _ => {}
    }
    if filters.tag.is_some() || filters.comment.is_some() {
        sql.push(" AND EXISTS(SELECT 1 FROM annotations a WHERE ")
            .push(NOTE_SCOPE);
        if let Some(tag) = &filters.tag {
            sql.push(" AND a.tags @> ").push_bind(vec![tag.clone()]);
        }
        if let Some(comment) = &filters.comment {
            sql.push(" AND lower(a.comment) LIKE ")
                .push_bind(literal_pattern(comment))
                .push(" ESCAPE '\\'");
        }
        sql.push(")");
    }
    if let Some(q) = &filters.q {
        let pattern = literal_pattern(q);
        sql.push(" AND (lower(m.white || ' ' || m.black || ' ' || m.filename || ' ' || m.display_name || ' ' || coalesce(m.tournament,'') || ' ' || coalesce(m.round,'') || ' ' || p.position_id) LIKE ").push_bind(pattern.clone()).push(" ESCAPE '\\' OR EXISTS(SELECT 1 FROM annotations a WHERE ").push(NOTE_SCOPE).push(" AND a.search_text LIKE ").push_bind(pattern).push(" ESCAPE '\\'))");
    }
    if let Some(state) = filters.analysis_state {
        sql.push(" AND ");
        if state == PositionAnalysisState::Missing {
            sql.push("NOT ");
        }
        sql.push(READY);
    }
    if let Some(loss) = filters.min_loss {
        sql.push(" AND ").push(LOSS).push(">=").push_bind(loss);
    }
    if let Some(loss) = filters.max_loss {
        sql.push(" AND ").push(LOSS).push("<=").push_bind(loss);
    }
    if let Some(anchor) = anchor {
        // Anchor lookup and page read share one filtered snapshot. A left-joined
        // marker distinguishes an exhausted page from a deleted/out-of-scope
        // anchor without running a less restrictive second lookup.
        sql.push("),anchor AS (SELECT * FROM filtered WHERE sort_id=")
            .push_bind(anchor)
            .push("),ranked AS (SELECT f.* FROM filtered f CROSS JOIN anchor a WHERE ((f.sort_value IS NULL AND a.sort_value IS NOT NULL) OR (f.sort_value IS NOT NULL AND a.sort_value IS NOT NULL AND f.sort_value ")
            .push(if direction == "ASC" { ">" } else { "<" })
            .push(" a.sort_value) OR (f.sort_value IS NOT DISTINCT FROM a.sort_value AND (f.sort_name,f.sort_match_id,f.sort_ordinal,f.sort_id)>(a.sort_name,a.sort_match_id,a.sort_ordinal,a.sort_id)))");
        table_order(sql, "f", direction);
        sql.push(" LIMIT ").push_bind(limit + 1).push(")");
        push_page(sql);
        sql.push(" SELECT page.*,EXISTS(SELECT 1 FROM anchor) AS anchor_exists FROM (VALUES (1)) marker(n) LEFT JOIN page ON true");
    } else if let Some(offset) = offset {
        sql.push("),totals AS (SELECT count(*) AS total FROM filtered),ranked AS (SELECT f.* FROM filtered f");
        table_order(sql, "f", direction);
        sql.push(" LIMIT ")
            .push_bind(limit + 1)
            .push(" OFFSET ")
            .push_bind(offset)
            .push(")");
        push_page(sql);
        sql.push(" SELECT page.*,totals.total FROM totals LEFT JOIN page ON true");
    } else {
        sql.push("),ranked AS (SELECT f.* FROM filtered f");
        table_order(sql, "f", direction);
        sql.push(" LIMIT ").push_bind(limit + 1).push(")");
        push_page(sql);
        sql.push(" SELECT page.* FROM page");
    }
    table_order(sql, "page", direction);
}

/// The hit columns of the `ranked` rows. Explicit summary fields keep source
/// SGF and original bytes out of list reads.
fn push_page(sql: &mut QueryBuilder<'_, Postgres>) {
    sql.push(",page AS (SELECT to_jsonb(p) AS position,jsonb_build_object('id',m.id,'source_available',NOT m.collection_only,'filename',m.filename,'format',m.format,'white',m.white,'black',m.black,'match_length',m.match_length,'position_count',m.position_count,'created_at',m.created_at) AS match_info,NOT m.collection_only AS source_available,coalesce(m.source_match_no,m.match_no) AS match_no,coalesce(m.source_database_id,m.database_id) AS database_id,coalesce(m.source_database_name,d.name) AS database_name,coalesce(nullif(m.display_name,''),m.filename) AS display_name,m.tournament,m.round,native.job_id,imported.imported_id,coalesce(native.engine,imported.engine) AS engine,coalesce(native.plies,(imported.projection->>'plies')::integer) AS plies,imported.projection->>'depth' AS depth,");
    sql.push(LOSS).push(" AS equity_loss,");
    sql.push(READY).push(" AS card_ready,r.sort_value,r.sort_name,r.sort_match_id,r.sort_ordinal,r.sort_id FROM ranked r JOIN positions p ON p.id=r.sort_id JOIN matches m ON m.id=p.match_uuid JOIN databases d ON d.id=m.database_id");
    push_native_analysis(sql);
    sql.push(")");
}

/// Match, game, position and move notes of every position on the page in one
/// query, in the same order per position as before.
pub(super) async fn load_notes(pool: &PgPool, items: &mut [PositionBrowserHit]) -> ApiResult<()> {
    if items.is_empty() {
        return Ok(());
    }
    let ids: Vec<Uuid> = items.iter().map(|hit| hit.position.id).collect();
    let rows = sqlx::query(&format!(
        "SELECT p.id,a.target_kind,a.tags,left(a.comment,280) AS comment_excerpt FROM positions p LEFT JOIN match_games g ON g.match_uuid=p.match_uuid AND g.game_no=p.game_no JOIN annotations a ON {NOTE_SCOPE} WHERE p.id=ANY($1) ORDER BY p.id,a.target_kind,a.id"
    ))
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let mut notes = HashMap::<Uuid, Vec<PositionBrowserNote>>::new();
    for row in rows {
        notes
            .entry(row.get("id"))
            .or_default()
            .push(PositionBrowserNote {
                scope: row.get("target_kind"),
                tags: row.get("tags"),
                comment_excerpt: row.get("comment_excerpt"),
            });
    }
    for hit in items {
        hit.notes = notes.remove(&hit.position.id).unwrap_or_default();
        hit.tags = hit
            .notes
            .iter()
            .flat_map(|note| note.tags.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
    }
    Ok(())
}

pub(super) async fn load_deck_memberships(
    pool: &PgPool,
    items: &mut [PositionBrowserHit],
) -> ApiResult<()> {
    if items.is_empty() {
        return Ok(());
    }
    let ids: Vec<Uuid> = items.iter().map(|hit| hit.position.id).collect();
    let rows: Vec<(Uuid, Uuid, String)> = sqlx::query_as(&format!(
        "SELECT p.id,dk.id,dk.name FROM positions p JOIN learning_cards c ON c.signature={CHECKER_SIGNATURE} JOIN deck_cards dc ON dc.card_id=c.id JOIN decks dk ON dk.id=dc.deck_id WHERE p.id=ANY($1) ORDER BY lower(dk.name) COLLATE \"C\",dk.name COLLATE \"C\",dk.id"
    )).bind(ids).fetch_all(pool).await.map_err(anyhow::Error::from)?;
    let mut memberships = HashMap::<Uuid, Vec<PositionDeckMembership>>::new();
    for (position, id, name) in rows {
        memberships
            .entry(position)
            .or_default()
            .push(PositionDeckMembership { id, name });
    }
    for hit in items {
        hit.decks = memberships.remove(&hit.position.id).unwrap_or_default();
    }
    Ok(())
}
