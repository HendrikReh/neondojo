//! Read-only, bounded review of pinned native checkpoints and validated source evaluations.
use crate::json_errors::{Json, Path, Query};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    configuration::ErrorThresholds,
    import::MatchFormat,
    imported_analysis::ImportedAnalysisData,
    match_analysis::Decision,
    model::{Job, MoveSeverity},
    move_catalog::StoredMove,
    recurring_mistakes::{imported_projection, source},
};
use axum::{Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection, Postgres, postgres::PgArguments, query::QueryAs};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

mod hits;
mod projection;
mod routes;
pub use hits::refresh_hits;
use hits::*;
pub(crate) use hits::{HitRefreshQueue, drain_hit_queue};
use projection::*;
pub use routes::{ReviewDoc, routes};

// Native decision numbering includes skipped doubles that share a source
// checker event. Recover the source ordinal before filtering out any decisions.
const REVIEW_SQL: &str = r#"
        WITH native_order AS (
          SELECT a.*,
            a.decision_no - count(*) FILTER (WHERE a.kind='cube' AND a.data->>'played'='no double')
              OVER (PARTITION BY a.game_no ORDER BY a.decision_no ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING)
              AS event_no,
            row_number() OVER (PARTITION BY a.game_no ORDER BY a.decision_no) AS sequence_no
          FROM move_analyses a JOIN match_analysis_games c ON c.job_id=a.job_id AND c.game_no=a.game_no
          WHERE a.job_id=$2 AND ($3::integer IS NULL OR a.game_no=$3)
        ), rows AS (
          SELECT 'native:'||a.id::text AS id,a.game_no,a.decision_no,a.kind,a.position_uuid,a.data,
                 to_jsonb(e) AS event,coalesce(p.ply,ep.ply) AS checker_ply,true AS native,a.player,a.equity_loss,
                 to_jsonb(p) AS position,e.move_no AS source_move_no
          FROM native_order a
          LEFT JOIN positions p ON p.id=a.position_uuid AND p.match_uuid=$1 AND p.game_no=a.game_no
          LEFT JOIN match_moves e ON a.kind='cube' AND a.sequence_no=a.decision_no
            AND e.match_uuid=$1 AND e.game_no=a.game_no AND e.move_no=a.event_no
          LEFT JOIN positions ep ON ep.id=e.position_uuid AND ep.match_uuid=$1 AND ep.game_no=a.game_no
          UNION ALL
          SELECT 'imported:'||a.id::text,a.game_no,a.source_move_no,a.kind,a.position_uuid,a.data,
                 to_jsonb(e),p.ply,false,e.player,NULL::double precision,NULL::jsonb,e.move_no
          FROM primary_imported_analyses a
          JOIN match_moves e ON e.match_uuid=a.match_uuid AND e.game_no=a.game_no AND e.move_no=a.source_move_no
          LEFT JOIN positions p ON p.id=e.position_uuid AND p.match_uuid=e.match_uuid AND p.game_no=e.game_no
          WHERE a.match_uuid=$1 AND a.kind IN ('checker','cube')
            AND a.position_uuid IS NOT DISTINCT FROM e.position_uuid
            AND $11
        ), slice AS (
          SELECT * FROM rows
          WHERE ($3::integer IS NULL OR game_no=$3) AND ($4::integer IS NULL OR player=$4)
            AND ($5::text IS NULL OR kind=$5)
            AND ($6::integer IS NULL OR (game_no,decision_no,kind='checker',id)>($6,$7,$8,$9))
            AND ($12::text[] IS NULL OR id=ANY($12))
          ORDER BY game_no,decision_no,(kind='checker'),id LIMIT $10
        )
        SELECT s.*, g.cube_enabled, nxt.ply AS next_checker_no
        FROM slice s LEFT JOIN match_games g ON g.match_uuid=$1 AND g.game_no=s.game_no
        LEFT JOIN LATERAL (
          SELECT p.ply FROM match_moves e JOIN positions p ON p.id=e.position_uuid
          WHERE s.kind='cube' AND s.checker_ply IS NULL AND e.match_uuid=$1
            AND e.game_no=s.game_no AND e.move_no>s.source_move_no AND e.kind='checker'
          ORDER BY e.move_no LIMIT 1
        ) nxt ON true
        ORDER BY s.game_no,s.decision_no,(s.kind='checker'),s.id
    "#;

const SCAN_LIMIT: i64 = 512;
/// Bump when `project` accepts or rejects other rows, derives another loss, or
/// changes the `hits_fingerprint` representation.
const HITS_VERSION: i32 = 3;
/// Numbered pages up to this size use the narrow hit set.
const HIT_PAGE_LIMIT: i64 = 100;

#[derive(Clone, Debug, Default, Deserialize, Serialize, IntoParams)]
#[into_params(parameter_in=Query)]
pub struct ReviewQuery {
    /// Legacy pinned job identity. Does not fall back to other evaluations.
    pub job_id: Option<Uuid>,
    pub analysis_id: Option<String>,
    pub game_no: Option<i32>,
    /// 0 = White, 1 = Black.
    pub player: Option<i32>,
    /// checker or cube.
    pub kind: Option<String>,
    pub severity: Option<MoveSeverity>,
    #[serde(default)]
    pub errors_only: bool,
    /// 1–100, default 50.
    pub limit: Option<i64>,
    /// Opaque continuation; preserve filters. Restart if the job's checkpoints change.
    pub after: Option<String>,
    /// One-based result page. Mutually exclusive with `after`.
    pub page: Option<i64>,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct ReviewDecision {
    /// Source-qualified identity, independent of pagination.
    pub id: String,
    pub position_uuid: Option<Uuid>,
    /// Source event for cube review; a skipped double links its checker event.
    pub move_id: Option<Uuid>,
    /// Exact cube context, including the position before the roll for a skipped double.
    pub review_position: Option<crate::model::Position>,
    /// Next checker move for an explicit cube event; null after the last checker move.
    pub next_checker_no: Option<i32>,
    pub game_no: i32,
    #[serde(flatten)]
    pub decision: Decision,
    pub source: crate::move_assessments::AssessmentSource,
    pub engine: String,
    pub depth: Option<String>,
    pub depth_plies: Option<i32>,
    pub job_id: Option<Uuid>,
}
#[derive(Serialize, ToSchema)]
pub struct ReviewDecisionPage {
    pub items: Vec<ReviewDecision>,
    /// A bounded scan can return an empty page with a continuation. Follow it until null.
    pub next_cursor: Option<String>,
    /// Total matching decisions when `page` is supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
}
#[derive(Serialize, Deserialize)]
struct Cursor {
    scope: String,
    game: i32,
    decision: i32,
    kind: String,
    id: String,
}
#[derive(FromRow)]
struct Row {
    cube_enabled: Option<bool>,
    id: String,
    game_no: i32,
    decision_no: i32,
    kind: String,
    position_uuid: Option<Uuid>,
    data: serde_json::Value,
    event: Option<serde_json::Value>,
    checker_ply: Option<i32>,
    next_checker_no: Option<i32>,
    native: bool,
    player: i32,
    equity_loss: Option<f64>,
    position: Option<serde_json::Value>,
}

fn review_rows_query<'q>() -> QueryAs<'q, Postgres, Row, PgArguments> {
    // The analysis source, cursor and page IDs can change the join plan.
    // Keep these plans specific to each request's values.
    sqlx::query_as(REVIEW_SQL).persistent(false)
}

pub(crate) async fn load_review(
    state: &AppState,
    id: Uuid,
    query: ReviewQuery,
) -> ApiResult<ReviewDecisionPage> {
    let (limit, offset) = review_bounds(&query, 100)?;
    let mut tx = crate::database_pool::begin_read_snapshot(&state.pool).await?;
    require_cataloged_on(state, &mut tx, id).await?;
    let (page, stale_analysis) = read_review(&mut tx, id, &query, limit, offset, true).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    if let Some(analysis_id) = stale_analysis {
        state.review_hit_queue.enqueue(id, &analysis_id);
        state.browse_wake.notify_one();
    }
    Ok(page)
}

/// Batch consumers reuse the same projection inside the caller's read-only
/// snapshot. Callers must ensure the match's catalogs are ready first.
pub(crate) async fn load_all_review_on(
    connection: &mut PgConnection,
    id: Uuid,
    analysis_id: &str,
    game_no: Option<i32>,
    kind: Option<String>,
) -> ApiResult<Vec<ReviewDecision>> {
    read_all_review(
        connection,
        id,
        &all_review_query(analysis_id, game_no, kind),
    )
    .await
}

const ALL_REVIEW_MAX: i64 = 200_000;
fn all_review_query(analysis_id: &str, game_no: Option<i32>, kind: Option<String>) -> ReviewQuery {
    ReviewQuery {
        analysis_id: Some(analysis_id.into()),
        game_no,
        kind,
        page: Some(1),
        limit: Some(ALL_REVIEW_MAX),
        ..Default::default()
    }
}
async fn read_all_review(
    connection: &mut PgConnection,
    id: Uuid,
    query: &ReviewQuery,
) -> ApiResult<Vec<ReviewDecision>> {
    let (limit, offset) = review_bounds(query, ALL_REVIEW_MAX)?;
    let (page, _) = read_review(connection, id, query, limit, offset, false).await?;
    if page.total.is_some_and(|total| total > ALL_REVIEW_MAX) {
        return Err(ApiError::invalid(
            "Analysis exceeds supported decision count",
        ));
    }
    Ok(page.items)
}

/// Read-only check that the match exists, is reviewable, and has its move and
/// imported-analysis catalogs populated. Wakes `browse_indexes::worker` and
/// returns `503 index_preparing` for uncataloged legacy matches.
pub(crate) async fn require_cataloged_on(
    state: &AppState,
    connection: &mut PgConnection,
    id: Uuid,
) -> ApiResult<()> {
    crate::play::require_review_available_on(&mut *connection, id).await?;
    let flags: Option<(bool, bool)> =
        sqlx::query_as("SELECT moves_cataloged,imported_analysis_loaded FROM matches WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(anyhow::Error::from)?;
    let Some((moves_cataloged, imported_loaded)) = flags else {
        return Err(ApiError::missing());
    };
    if !moves_cataloged || !imported_loaded {
        return Err(crate::browse_indexes::preparing(
            state,
            "The match analysis catalog",
        ));
    }
    Ok(())
}

fn review_bounds(query: &ReviewQuery, max_limit: i64) -> ApiResult<(i64, Option<i64>)> {
    let limit = query.limit.unwrap_or(50);
    if !(1..=max_limit).contains(&limit)
        || query.game_no.is_some_and(|g| g <= 0)
        || query.player.is_some_and(|p| !(0..=1).contains(&p))
        || query
            .kind
            .as_deref()
            .is_some_and(|k| !matches!(k, "checker" | "cube"))
    {
        return Err(ApiError::invalid(
            "Invalid review limit, game, player or kind",
        ));
    }
    let offset = crate::search::page_offset(query.page, query.after.is_some(), limit)?;
    Ok((limit, offset))
}

struct ReviewScope {
    format: MatchFormat,
    analysis_id: Option<String>,
    imported: bool,
    job: Option<Box<Job>>,
    game: Option<i32>,
    thresholds: ErrorThresholds,
    scope: String,
}

async fn resolve_review_scope(
    tx: &mut PgConnection,
    id: Uuid,
    query: &ReviewQuery,
) -> ApiResult<ReviewScope> {
    let (format, revision): (MatchFormat, i64) =
        sqlx::query_as("SELECT format,source_revision FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    let legacy = query.job_id.map(|id| format!("job:{id}"));
    if query.analysis_id.is_some()
        && legacy
            .as_ref()
            .is_some_and(|id| Some(id) != query.analysis_id.as_ref())
    {
        return Err(ApiError::invalid(
            "analysis_id and job_id must identify the same result",
        ));
    }
    let selected = crate::analysis_catalog::resolve_selection_on(
        &mut *tx,
        id,
        query.analysis_id.as_deref().or(legacy.as_deref()),
    )
    .await?;
    let analysis_id = selected.as_ref().map(|s| s.id());
    let imported = matches!(
        selected,
        Some(crate::analysis_catalog::SelectedAnalysis::Imported { .. })
    );
    let job = match selected {
        Some(crate::analysis_catalog::SelectedAnalysis::Job(j)) => Some(j),
        _ => None,
    };
    if query.job_id.is_some()
        && query
            .game_no
            .zip(job.as_ref().and_then(|j| j.game_no))
            .is_some_and(|(a, b)| a != b)
    {
        return Err(ApiError::invalid("game_no does not match this job's scope"));
    }
    let job_id = job.as_ref().map(|j| j.id);
    let game = query
        .game_no
        .or_else(|| job.as_ref().and_then(|j| j.game_no));
    if let Some(game) = game {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM match_games WHERE match_uuid=$1 AND game_no=$2)",
        )
        .bind(id)
        .bind(game)
        .fetch_one(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
        if !exists {
            return Err(ApiError::missing());
        }
    }
    let checkpoints: i64 =
        sqlx::query_scalar("SELECT count(*) FROM match_analysis_games WHERE job_id=$1")
            .bind(job_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    let thresholds = crate::configuration::store::error_thresholds(&mut *tx).await?;
    let scope = serde_json::to_string(&(
        id,
        revision,
        &analysis_id,
        game,
        query.player,
        &query.kind,
        query.severity,
        query.errors_only,
        thresholds,
        checkpoints,
        "cube-before-checker",
    ))
    .map_err(anyhow::Error::from)?;
    Ok(ReviewScope {
        format,
        analysis_id,
        imported,
        job,
        game,
        thresholds,
        scope,
    })
}

fn decode_review_cursor(after: Option<&str>, scope: &str) -> ApiResult<Option<Cursor>> {
    const INVALID: &str =
        "Invalid review cursor or changed filters/checkpoints; restart pagination";
    let Some(after) = after else {
        return Ok(None);
    };
    let cursor: Cursor = crate::cursor::decode(after, crate::cursor::DEFAULT_MAX_LEN, INVALID)?;
    if cursor.scope != scope {
        return Err(ApiError::invalid(INVALID));
    }
    Ok(Some(cursor))
}

async fn load_single_position_review(
    tx: &mut PgConnection,
    id: Uuid,
    query: &ReviewQuery,
    job: &Job,
    game: Option<i32>,
    thresholds: ErrorThresholds,
    offset: Option<i64>,
) -> ApiResult<ReviewDecisionPage> {
    let position = sqlx::query_as::<_, crate::model::Position>(
        "SELECT * FROM positions WHERE id=$1 AND match_uuid=$2",
    )
    .bind(job.position_uuid)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let mut items = Vec::new();
    if let Some(position) = position.filter(|p| game.is_none_or(|g| g == p.game_no))
        && job.status == "completed"
        && let Some(analysis) = job
            .result
            .as_ref()
            .and_then(|r| serde_json::from_value::<crate::engine::Analysis>(r.clone()).ok())
        && analysis.plies == job.plies
        && let Some(mut decision) = crate::move_assessments::hint_decision(&position, &analysis)
    {
        let severity = if decision.evaluation_status == "evaluated" {
            thresholds.classify(decision.equity_loss)
        } else {
            MoveSeverity::None
        };
        decision.severity = severity.as_str().into();
        if query.player.is_none_or(|p| p == position.player)
            && query.kind.as_deref().is_none_or(|k| k == "checker")
            && (!query.errors_only || severity != MoveSeverity::None)
            && query.severity.is_none_or(|s| s == severity)
        {
            items.push(ReviewDecision {
                id: format!("hint:{}", job.id),
                position_uuid: Some(position.id),
                move_id: None,
                review_position: None,
                next_checker_no: None,
                game_no: position.game_no,
                decision,
                source: crate::move_assessments::AssessmentSource::Native,
                engine: analysis.engine,
                depth: Some(format!("{}-ply", job.plies)),
                depth_plies: Some(job.plies),
                job_id: Some(job.id),
            });
        }
    }
    let total = items.len() as i64;
    if offset.is_some_and(|offset| offset > 0) || query.after.is_some() {
        items.clear();
    }
    Ok(ReviewDecisionPage {
        items,
        next_cursor: None,
        total: query.page.map(|_| total),
    })
}

enum HitPageLookup {
    Hit(ReviewDecisionPage),
    Stale(String),
    Skip,
}

async fn try_load_hit_page(
    tx: &mut PgConnection,
    id: Uuid,
    query: &ReviewQuery,
    resolved: &ReviewScope,
    offset: i64,
    limit: i64,
    track_stale: bool,
) -> ApiResult<HitPageLookup> {
    let Some(analysis_id) = resolved.analysis_id.as_deref() else {
        return Ok(HitPageLookup::Skip);
    };
    let job_id = resolved.job.as_ref().map(|j| j.id);
    let current = hits_fingerprint(&mut *tx, id, analysis_id, job_id).await?;
    let stored: Option<String> = sqlx::query_scalar(
        "SELECT fingerprint FROM review_hit_sets WHERE match_id=$1 AND analysis_id=$2",
    )
    .bind(id)
    .bind(analysis_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if stored.as_deref() == Some(current.as_str()) {
        let page = HitPage {
            match_id: id,
            analysis_id,
            job_id,
            imported: resolved.imported,
            format: resolved.format,
            job: resolved.job.as_deref(),
            game: resolved.game,
            query,
            thresholds: resolved.thresholds,
            offset,
            limit,
        };
        if let Some(page) = page.load(&mut *tx).await? {
            return Ok(HitPageLookup::Hit(page));
        }
    }
    if track_stale {
        Ok(HitPageLookup::Stale(analysis_id.to_owned()))
    } else {
        Ok(HitPageLookup::Skip)
    }
}

async fn scan_review_pages(
    tx: &mut PgConnection,
    id: Uuid,
    query: &ReviewQuery,
    resolved: &ReviewScope,
    mut cursor: Option<Cursor>,
    limit: i64,
    offset: Option<i64>,
) -> ApiResult<ReviewDecisionPage> {
    let job_id = resolved.job.as_ref().map(|j| j.id);
    let mut items = Vec::new();
    let mut matched = 0;
    loop {
        let rows = review_rows_query()
            .bind(id)
            .bind(job_id)
            .bind(resolved.game)
            .bind(query.player)
            .bind(&query.kind)
            .bind(cursor.as_ref().map(|c| c.game))
            .bind(cursor.as_ref().map(|c| c.decision))
            .bind(cursor.as_ref().map(|c| c.kind == "checker"))
            .bind(cursor.as_ref().map(|c| &c.id))
            .bind(SCAN_LIMIT + 1)
            .bind(resolved.imported)
            .bind(None::<Vec<String>>)
            .fetch_all(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
        let job = resolved.job.clone();
        let scope = resolved.scope.clone();
        let format = resolved.format;
        let thresholds = resolved.thresholds;
        let severity = query.severity;
        let errors_only = query.errors_only;
        let (batch, continuation, count) = tokio::task::spawn_blocking(move || {
            let mut batch = Vec::new();
            let count = rows.len();
            let mut last = None;
            let mut inspected = 0;
            let mut matched_in_batch = 0;
            for row in rows.into_iter().take(SCAN_LIMIT as usize) {
                if offset.is_none() && batch.len() == limit as usize {
                    break;
                }
                inspected += 1;
                last = Some(Cursor {
                    scope: scope.clone(),
                    game: row.game_no,
                    decision: row.decision_no,
                    kind: row.kind.clone(),
                    id: row.id.clone(),
                });
                let Some(result) = filtered_decision(
                    row,
                    id,
                    format,
                    job.as_deref(),
                    errors_only,
                    severity,
                    thresholds,
                ) else {
                    continue;
                };
                let index = matched + matched_in_batch;
                if offset.is_none_or(|offset| index >= offset && index - offset < limit) {
                    batch.push(result);
                }
                matched_in_batch += 1;
            }
            (batch, last.filter(|_| inspected < count), matched_in_batch)
        })
        .await
        .map_err(anyhow::Error::from)?;
        items.extend(batch);
        matched += count;
        if offset.is_none() || continuation.is_none() {
            let next_cursor = continuation
                .map(|c| crate::cursor::encode(&c))
                .transpose()?;
            return Ok(ReviewDecisionPage {
                items,
                next_cursor,
                total: offset.map(|_| matched),
            });
        }
        cursor = continuation;
    }
}

async fn read_review(
    tx: &mut PgConnection,
    id: Uuid,
    query: &ReviewQuery,
    limit: i64,
    offset: Option<i64>,
    track_stale: bool,
) -> ApiResult<(ReviewDecisionPage, Option<String>)> {
    let resolved = resolve_review_scope(&mut *tx, id, query).await?;
    let cursor = decode_review_cursor(query.after.as_deref(), &resolved.scope)?;
    if let Some(job) = resolved.job.as_ref().filter(|j| j.kind == "analysis") {
        let page = load_single_position_review(
            &mut *tx,
            id,
            query,
            job,
            resolved.game,
            resolved.thresholds,
            offset,
        )
        .await?;
        return Ok((page, None));
    }
    // Numbered pages count and select in the narrow hit set under this read
    // snapshot and project only their own decisions. When the hit set is absent
    // or stale, answer from the same read-only snapshot via scan and let the
    // background worker rebuild the hit set outside the GET request.
    let mut stale = None;
    if let Some(offset) = offset
        && limit <= HIT_PAGE_LIMIT
    {
        match try_load_hit_page(&mut *tx, id, query, &resolved, offset, limit, track_stale).await? {
            HitPageLookup::Hit(page) => return Ok((page, None)),
            HitPageLookup::Stale(analysis_id) => stale = Some(analysis_id),
            HitPageLookup::Skip => {}
        }
    }
    let page = scan_review_pages(&mut *tx, id, query, &resolved, cursor, limit, offset).await?;
    Ok((page, stale))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires PostgreSQL; measures a real XG match in an isolated database"]
    async fn explain_real_xg_review_page(pool: sqlx::PgPool) {
        let bytes = include_bytes!("../tests/fixtures/WC2014.xg");
        let sgf = crate::import::xg_to_sgf(bytes).unwrap();
        let parsed = crate::import::parse_sgf(&sgf).unwrap();
        let id = crate::store::import_match(&pool, "WC2014.xg", "xg", bytes, &sgf, &parsed)
            .await
            .unwrap()
            .r#match
            .id;
        crate::move_catalog::ensure_catalog(&pool, id)
            .await
            .unwrap();
        crate::imported_analysis::ensure_catalog(&pool, id)
            .await
            .unwrap();
        sqlx::query("ANALYZE imported_analyses")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("ANALYZE match_moves")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("ANALYZE positions")
            .execute(&pool)
            .await
            .unwrap();
        let (games,records):(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM match_games WHERE match_uuid=$1),(SELECT count(*) FROM imported_analyses WHERE match_uuid=$1)").bind(id).fetch_one(&pool).await.unwrap();
        let plan: serde_json::Value = sqlx::query_scalar(&format!(
            "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {REVIEW_SQL}"
        ))
        .bind(id)
        .bind(None::<Uuid>)
        .bind(Some(1_i32))
        .bind(Some(0_i32))
        .bind(Some("checker"))
        .bind(None::<i32>)
        .bind(None::<i32>)
        .bind(None::<bool>)
        .bind(None::<String>)
        .bind(SCAN_LIMIT + 1)
        .bind(true)
        .bind(None::<Vec<String>>)
        .fetch_one(&pool)
        .await
        .unwrap();
        eprintln!(
            "XG review query: games={games}, imported_records={records}, filter game=1/player=0/kind=checker; plan={plan}"
        );
        assert!(records > 100);
        let rows = plan[0]["Plan"]["Actual Rows"].as_i64().unwrap();
        assert!(rows > 0 && rows <= SCAN_LIMIT + 1);
    }
}
