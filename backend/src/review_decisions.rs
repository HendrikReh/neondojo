//! Read-only, bounded review of pinned native checkpoints and validated source evaluations.
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    imported_analysis::ImportedAnalysisData,
    match_analysis::Decision,
    model::{Job, MoveSeverity},
    move_catalog::StoredMove,
    recurring_mistakes::{imported_projection, source},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

const REVIEW_SQL: &str = r#"
        WITH rows AS (
          SELECT 'native:'||a.id::text AS id,a.game_no,a.decision_no,a.kind,a.position_uuid,a.data,
                 NULL::jsonb AS event,NULL::integer AS checker_ply,true AS native,a.player,a.equity_loss,to_jsonb(p) AS position
          FROM move_analyses a JOIN match_analysis_games c ON c.job_id=a.job_id AND c.game_no=a.game_no
          LEFT JOIN positions p ON p.id=a.position_uuid AND p.match_uuid=$1 AND p.game_no=a.game_no
          WHERE a.job_id=$2
          UNION ALL
          SELECT 'imported:'||a.id::text,a.game_no,a.source_move_no,a.kind,a.position_uuid,a.data,
                 to_jsonb(e),p.ply,false,e.player,NULL::double precision,NULL::jsonb
          FROM imported_analyses a
          JOIN match_moves e ON e.match_uuid=a.match_uuid AND e.game_no=a.game_no AND e.move_no=a.source_move_no
          LEFT JOIN positions p ON p.id=e.position_uuid AND p.match_uuid=e.match_uuid AND p.game_no=e.game_no
          WHERE a.match_uuid=$1 AND a.kind IN ('checker','cube')
            AND a.position_uuid IS NOT DISTINCT FROM e.position_uuid
            AND NOT EXISTS(SELECT 1 FROM match_analysis_games c WHERE c.job_id=$2 AND c.game_no=a.game_no)
        )
        SELECT id,game_no,decision_no,kind,position_uuid,data,event,checker_ply,native,player,equity_loss,position FROM rows
        WHERE ($3::integer IS NULL OR game_no=$3) AND ($4::integer IS NULL OR player=$4)
          AND ($5::text IS NULL OR kind=$5)
          AND ($6::integer IS NULL OR (game_no,decision_no,kind,id)>($6,$7,$8,$9))
        ORDER BY game_no,decision_no,kind,id LIMIT $10
    "#;

const SCAN_LIMIT: i64 = 512;

#[derive(Debug, Deserialize, Serialize, IntoParams)]
#[into_params(parameter_in=Query)]
pub struct ReviewQuery {
    /// Optional pinned match-analysis job; imported data covers its uncheckpointed games.
    pub job_id: Option<Uuid>,
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
    /// Actual source event for cube navigation; virtual no-double links its checker event.
    pub move_id: Option<Uuid>,
    /// Exact source cube context, including pre-roll dice for a virtual no-double.
    pub review_position: Option<crate::model::Position>,
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
    id: String,
    game_no: i32,
    decision_no: i32,
    kind: String,
    position_uuid: Option<Uuid>,
    data: serde_json::Value,
    event: Option<serde_json::Value>,
    checker_ply: Option<i32>,
    native: bool,
    player: i32,
    equity_loss: Option<f64>,
    position: Option<serde_json::Value>,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/matches/{id}/review-decisions", get(review))
}
#[utoipa::path(get,path="/api/matches/{id}/review-decisions",params(("id"=Uuid,Path),ReviewQuery),responses((status=200,body=ReviewDecisionPage),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Match analysis")]
async fn review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<ReviewQuery>, axum::extract::rejection::QueryRejection>,
) -> ApiResult<Json<ReviewDecisionPage>> {
    let Query(query) = query.map_err(|e| ApiError::invalid(e.body_text()))?;
    let limit = query.limit.unwrap_or(50);
    if !(1..=100).contains(&limit)
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
    crate::play::require_review_available(&state.pool, id).await?;
    if !crate::move_catalog::ensure_catalog(&state.pool, id).await?
        || !crate::imported_analysis::ensure_catalog(&state.pool, id).await?
    {
        return Err(ApiError::missing());
    }
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let (format, revision): (String, i64) =
        sqlx::query_as("SELECT format,source_revision FROM matches WHERE id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    let job = if let Some(job_id) = query.job_id {
        let job: Job = sqlx::query_as("SELECT * FROM jobs WHERE id=$1")
            .bind(job_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?
            .ok_or_else(ApiError::missing)?;
        let job_revision: i64 = sqlx::query_scalar("SELECT source_revision FROM jobs WHERE id=$1")
            .bind(job_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
        if job.kind != "match"
            || job.match_uuid != Some(id)
            || job_revision != revision
            || query.game_no.zip(job.game_no).is_some_and(|(a, b)| a != b)
        {
            return Err(ApiError::invalid(
                "job_id must refer to the current match source and requested scope",
            ));
        }
        Some(job)
    } else {
        None
    };
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
            .bind(query.job_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    let scope = serde_json::to_string(&(
        id,
        revision,
        query.job_id,
        game,
        query.player,
        &query.kind,
        query.severity,
        query.errors_only,
        checkpoints,
    ))
    .map_err(anyhow::Error::from)?;
    let cursor = if let Some(after) = &query.after {
        let invalid = || {
            ApiError::invalid(
                "Invalid review cursor or changed filters/checkpoints; restart pagination",
            )
        };
        if after.len() > 4096 {
            return Err(invalid());
        }
        let bytes = URL_SAFE_NO_PAD.decode(after).map_err(|_| invalid())?;
        let c: Cursor = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if c.scope != scope {
            return Err(invalid());
        }
        Some(c)
    } else {
        None
    };
    let mut cursor = cursor;
    let mut items = Vec::new();
    let mut matched = 0;
    loop {
        let rows: Vec<Row> = sqlx::query_as(REVIEW_SQL)
            .bind(id)
            .bind(query.job_id)
            .bind(game)
            .bind(query.player)
            .bind(&query.kind)
            .bind(cursor.as_ref().map(|c| c.game))
            .bind(cursor.as_ref().map(|c| c.decision))
            .bind(cursor.as_ref().map(|c| &c.kind))
            .bind(cursor.as_ref().map(|c| &c.id))
            .bind(SCAN_LIMIT + 1)
            .fetch_all(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
        let format = format.clone();
        let job = job.clone();
        let scope = scope.clone();
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
                let Some(result) =
                    filtered_decision(row, id, &format, job.as_ref(), errors_only, severity)
                else {
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
                .map(|c| serde_json::to_vec(&c).map(|bytes| URL_SAFE_NO_PAD.encode(bytes)))
                .transpose()
                .map_err(anyhow::Error::from)?;
            tx.commit().await.map_err(anyhow::Error::from)?;
            return Ok(Json(ReviewDecisionPage {
                items,
                next_cursor,
                total: offset.map(|_| matched),
            }));
        }
        cursor = continuation;
    }
}

fn filtered_decision(
    row: Row,
    id: Uuid,
    format: &str,
    job: Option<&Job>,
    errors_only: bool,
    filter: Option<MoveSeverity>,
) -> Option<ReviewDecision> {
    let mut result = project(row, id, format, job)?;
    let severity = MoveSeverity::from_normalized_loss(result.decision.equity_loss);
    if errors_only
        && (result.decision.evaluation_status != "evaluated"
            || result.decision.equity_loss < MoveSeverity::DOUBTFUL_MIN_LOSS)
        || filter.is_some_and(|s| s.as_str() != severity.as_str())
    {
        return None;
    }
    result.decision.severity = severity.as_str().into();
    Some(result)
}
fn project(row: Row, match_id: Uuid, format: &str, job: Option<&Job>) -> Option<ReviewDecision> {
    let (decision, move_id, review_position, engine, depth, depth_plies, job_id, source) = if row
        .native
    {
        let job = job?;
        let decision: Decision = serde_json::from_value(row.data).ok()?;
        if decision.decision_no != row.decision_no
            || decision.kind != row.kind
            || decision.player != row.player
            || row
                .equity_loss
                .is_none_or(|loss| !loss.is_finite() || (loss - decision.equity_loss).abs() > 1e-9)
        {
            return None;
        }
        let valid_status = match decision.evaluation_status.as_str() {
            "evaluated" => source::evaluated(&decision),
            "forced" => {
                decision.kind == "checker"
                    && decision.equity_loss == 0.0
                    && row
                        .position
                        .and_then(|value| {
                            serde_json::from_value::<crate::model::Position>(value).ok()
                        })
                        .is_some_and(|p| {
                            p.player == decision.player
                                && crate::move_assessments::after_play(&p, "Cannot move").is_some()
                        })
            }
            "unavailable" => decision.played_equity.is_none() && decision.best_equity.is_none(),
            _ => false,
        };
        if !valid_status {
            return None;
        }
        let saved_engine = job
            .engine_snapshot
            .as_ref()
            .map(|s| match s.engine_id.as_str() {
                "bgsage" => "Open Sage",
                "gnubg" => "GNU Backgammon",
                _ => s.engine_id.as_str(),
            })
            .unwrap_or("GNU Backgammon");
        let engine = job
            .result
            .as_ref()
            .and_then(|r| r["engine"].as_str())
            .unwrap_or(saved_engine)
            .to_owned();
        (
            decision,
            None,
            None,
            engine,
            Some(format!("{}-ply", job.plies)),
            Some(job.plies),
            Some(job.id),
            crate::move_assessments::AssessmentSource::Native,
        )
    } else {
        let event: StoredMove = serde_json::from_value(row.event?).ok()?;
        let source = imported_projection::source_move(&event, row.checker_ply)?;
        let record: ImportedAnalysisData = serde_json::from_value(row.data).ok()?;
        if record.kind != row.kind {
            return None;
        }
        let projected =
            imported_projection::project(&record, &source, match_id, row.position_uuid, format)?;
        let engine = record
            .engine_version
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .map_or_else(
                || record.engine.clone(),
                |v| format!("{} {v}", record.engine),
            );
        (
            projected.decision,
            Some(event.id),
            (record.kind == "cube").then_some(projected.position),
            engine,
            projected.depth,
            projected.depth_plies,
            None,
            crate::move_assessments::AssessmentSource::Imported,
        )
    };
    Some(ReviewDecision {
        id: row.id,
        position_uuid: row.position_uuid,
        move_id,
        review_position,
        game_no: row.game_no,
        decision,
        source,
        engine,
        depth,
        depth_plies,
        job_id,
    })
}
#[derive(OpenApi)]
#[openapi(paths(review), components(schemas(ReviewDecision, ReviewDecisionPage)))]
pub struct ReviewDoc;

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
        .bind(None::<String>)
        .bind(None::<String>)
        .bind(SCAN_LIMIT + 1)
        .fetch_one(&pool)
        .await
        .unwrap();
        eprintln!(
            "XG review query: games={games}, imported_records={records}, filter game=1/player=0/kind=checker; plan={plan}"
        );
        assert!(records > 100);
        assert!(plan[0]["Plan"]["Actual Rows"].as_i64().unwrap() <= SCAN_LIMIT + 1);
    }
}
