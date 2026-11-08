//! Read-only statistics for exactly one native analysis job and its durable games.
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    model::{Job, MatchAnalysisSummary, PlayerAnalysisSummary},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct JobAnalysisStatistics {
    pub job_id: Uuid,
    /// The authoritative persisted job summary, including partial progress; null before any summary exists.
    pub summary: Option<MatchAnalysisSummary>,
    /// Ordered source games in this job's scope. Coverage requires a committed game checkpoint.
    pub games: Vec<GameAnalysisStatistics>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GameAnalysisStatistics {
    pub game_no: i32,
    pub white_score: i32,
    pub black_score: i32,
    pub crawford: bool,
    /// True only when this source game has a durable, complete checkpoint for this job.
    pub analyzed: bool,
    /// Null before checkpointing. Legacy checkpoints provide decision totals but no native game PR.
    pub summary: Option<MatchAnalysisSummary>,
}

#[derive(FromRow)]
struct GameRow {
    game_no: i32,
    white_score: i32,
    black_score: i32,
    crawford: bool,
    position_count: i32,
    analyzed: bool,
    summary: Option<serde_json::Value>,
}

#[derive(FromRow)]
struct DecisionTotals {
    game_no: i32,
    player: i32,
    kind: String,
    decisions: i32,
    forced: i32,
    unavailable: i32,
    doubtful: i32,
    bad: i32,
    very_bad: i32,
    loss: f64,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/jobs/{id}/statistics", get(statistics))
}

#[utoipa::path(get, path="/api/jobs/{id}/statistics", params(("id"=Uuid, Path)), responses((status=200, body=JobAnalysisStatistics), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Match analysis")]
async fn statistics(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<JobAnalysisStatistics>> {
    // A worker may checkpoint while this request runs. All summary and coverage
    // reads must describe the same committed database snapshot.
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    let job: Job = sqlx::query_as("SELECT * FROM jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if job.kind != "match" {
        return Err(ApiError::invalid("This job is not a match analysis"));
    }
    let summary: Option<MatchAnalysisSummary> = job
        .result
        .map(serde_json::from_value)
        .transpose()
        .map_err(anyhow::Error::from)?;
    let mut rows: Vec<GameRow> = sqlx::query_as("SELECT g.game_no,g.white_score,g.black_score,g.crawford,g.position_count,c.job_id IS NOT NULL AS analyzed,c.summary FROM match_games g LEFT JOIN match_analysis_games c ON c.job_id=$1 AND c.game_no=g.game_no WHERE g.match_uuid=$2 AND ($3::integer IS NULL OR g.game_no=$3) ORDER BY g.game_no")
        .bind(id).bind(job.match_uuid).bind(job.game_no).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
    // Pre-catalog libraries still have immutable SGF and durable checkpoints.
    // Recover the source metadata in memory, keeping this endpoint read-only.
    if rows.is_empty() {
        let source: String = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1")
            .bind(job.match_uuid)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
        let checkpoints: Vec<(i32, Option<serde_json::Value>)> =
            sqlx::query_as("SELECT game_no,summary FROM match_analysis_games WHERE job_id=$1")
                .bind(id)
                .fetch_all(&mut *tx)
                .await
                .map_err(anyhow::Error::from)?;
        let games =
            tokio::task::spawn_blocking(move || crate::import::parse_sgf(&source).map(|m| m.games))
                .await
                .map_err(anyhow::Error::from)??;
        rows = games
            .into_iter()
            .filter(|g| job.game_no.is_none_or(|n| n == g.game_no))
            .map(|g| {
                let checkpoint = checkpoints.iter().find(|(n, _)| *n == g.game_no);
                GameRow {
                    game_no: g.game_no,
                    white_score: g.white_score,
                    black_score: g.black_score,
                    crawford: g.crawford,
                    position_count: g.position_count,
                    analyzed: checkpoint.is_some(),
                    summary: checkpoint.and_then(|(_, s)| s.clone()),
                }
            })
            .collect();
    }
    let totals: Vec<DecisionTotals> = sqlx::query_as("SELECT m.game_no,m.player,m.kind,count(*)::integer AS decisions,count(*) FILTER(WHERE m.data->>'evaluation_status'='forced')::integer AS forced,count(*) FILTER(WHERE m.data->>'evaluation_status'='unavailable')::integer AS unavailable,count(*) FILTER(WHERE m.data->>'severity'='doubtful')::integer AS doubtful,count(*) FILTER(WHERE m.data->>'severity'='bad')::integer AS bad,count(*) FILTER(WHERE m.data->>'severity'='very_bad')::integer AS very_bad,sum(m.equity_loss) AS loss FROM move_analyses m JOIN match_analysis_games c ON c.job_id=m.job_id AND c.game_no=m.game_no WHERE m.job_id=$1 AND c.summary IS NULL AND ($2::integer IS NULL OR m.game_no=$2) GROUP BY m.game_no,m.player,m.kind")
        .bind(id).bind(job.game_no).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    let games = rows
        .into_iter()
        .map(|row| {
            let game_summary = if !row.analyzed {
                None
            } else if let Some(saved) = row.summary {
                Some(serde_json::from_value(saved)?)
            } else {
                let mut legacy = MatchAnalysisSummary {
                    engine: summary.as_ref().map_or_else(
                        || "GNUbg (version unavailable)".into(),
                        |s| s.engine.clone(),
                    ),
                    plies: job.plies,
                    total_games: 1,
                    completed_games: 1,
                    total_moves: row.position_count,
                    analyzed_moves: 0,
                    players: [
                        PlayerAnalysisSummary::default(),
                        PlayerAnalysisSummary {
                            player: 1,
                            ..Default::default()
                        },
                    ],
                    performance_rating: None,
                };
                for total in totals.iter().filter(|t| t.game_no == row.game_no) {
                    let player = &mut legacy.players[total.player as usize];
                    if total.kind == "checker" {
                        player.checker_decisions += total.decisions;
                        player.checker_equity_loss += total.loss;
                        legacy.analyzed_moves += total.decisions;
                    } else {
                        player.cube_decisions += total.decisions;
                        player.cube_equity_loss += total.loss;
                    }
                    player.forced_moves += total.forced;
                    player.unavailable += total.unavailable;
                    player.doubtful += total.doubtful;
                    player.bad += total.bad;
                    player.very_bad += total.very_bad;
                }
                Some(legacy)
            };
            Ok(GameAnalysisStatistics {
                game_no: row.game_no,
                white_score: row.white_score,
                black_score: row.black_score,
                crawford: row.crawford,
                analyzed: row.analyzed,
                summary: game_summary,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Json(JobAnalysisStatistics {
        job_id: id,
        summary,
        games,
    }))
}

#[derive(OpenApi)]
#[openapi(
    paths(statistics),
    components(schemas(JobAnalysisStatistics, GameAnalysisStatistics))
)]
pub struct StatisticsDoc;
