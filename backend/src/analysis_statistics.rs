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
    /// Typed errors in checkpointed games; null when durable decision coverage cannot be verified.
    pub error_breakdown: Option<[PlayerErrorBreakdown; 2]>,
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
    /// Typed errors for this complete checkpoint; null for missing or inconsistent durable coverage.
    pub error_breakdown: Option<[PlayerErrorBreakdown; 2]>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct ErrorSeverityCounts {
    pub doubtful: i32,
    pub bad: i32,
    pub very_bad: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct PlayerErrorBreakdown {
    pub player: i32,
    pub checker: ErrorSeverityCounts,
    pub cube: ErrorSeverityCounts,
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
    errors_doubtful: i32,
    errors_bad: i32,
    errors_very_bad: i32,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/jobs/{id}/statistics", get(statistics))
}

#[utoipa::path(get, path="/api/jobs/{id}/statistics", params(("id"=Uuid, Path)), responses((status=200, body=JobAnalysisStatistics), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Match analysis")]
async fn statistics(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<JobAnalysisStatistics>> {
    // Complete the source visibility check before holding a pool connection.
    let visibility_job = crate::store::get_job(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_job_review_available(&state.pool, &visibility_job).await?;
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
    let current_source: bool = sqlx::query_scalar("SELECT j.source_revision=m.source_revision FROM jobs j JOIN matches m ON m.id=j.match_uuid WHERE j.id=$1")
        .bind(id).fetch_one(&mut *tx).await.map_err(anyhow::Error::from)?;
    if !current_source {
        return Err(ApiError::invalid(
            "The match advanced after this analysis; analyze the current match again",
        ));
    }
    let source_available: bool =
        sqlx::query_scalar("SELECT NOT collection_only FROM matches WHERE id=$1")
            .bind(job.match_uuid)
            .fetch_one(&mut *tx)
            .await
            .map_err(anyhow::Error::from)?;
    if !source_available {
        return Err(ApiError::invalid(
            "The source match was removed; retained decisions are position analysis only",
        ));
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
    let totals: Vec<DecisionTotals> = sqlx::query_as("SELECT m.game_no,m.player,m.kind,count(*)::integer AS decisions,count(*) FILTER(WHERE m.data->>'evaluation_status'='forced')::integer AS forced,count(*) FILTER(WHERE m.data->>'evaluation_status'='unavailable')::integer AS unavailable,count(*) FILTER(WHERE m.data->>'severity'='doubtful')::integer AS doubtful,count(*) FILTER(WHERE m.data->>'severity'='bad')::integer AS bad,count(*) FILTER(WHERE m.data->>'severity'='very_bad')::integer AS very_bad,sum(m.equity_loss) AS loss,count(*) FILTER(WHERE m.equity_loss>0 AND COALESCE(m.data->>'evaluation_status','') NOT IN ('forced','unavailable') AND m.data->>'severity'='doubtful')::integer AS errors_doubtful,count(*) FILTER(WHERE m.equity_loss>0 AND COALESCE(m.data->>'evaluation_status','') NOT IN ('forced','unavailable') AND m.data->>'severity'='bad')::integer AS errors_bad,count(*) FILTER(WHERE m.equity_loss>0 AND COALESCE(m.data->>'evaluation_status','') NOT IN ('forced','unavailable') AND m.data->>'severity'='very_bad')::integer AS errors_very_bad FROM move_analyses m JOIN match_analysis_games c ON c.job_id=m.job_id AND c.game_no=m.game_no WHERE m.job_id=$1 AND ($2::integer IS NULL OR m.game_no=$2) GROUP BY m.game_no,m.player,m.kind")
        .bind(id).bind(job.game_no).fetch_all(&mut *tx).await.map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    let all_totals: Vec<_> = totals.iter().collect();
    // Legacy checkpoints have no per-game cube declaration. Their typed data
    // is safe only while the job's declared coverage verifies all stored rows.
    let legacy_coverage = summary.as_ref().is_some_and(|summary| {
        summary.completed_games > 0
            && summary.completed_games as usize == rows.iter().filter(|row| row.analyzed).count()
            && covers_summary(summary, &all_totals)
    });
    let games = rows
        .into_iter()
        .map(|row| {
            let saved_summary = row.summary.is_some();
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
            let game_totals: Vec<_> = totals.iter().filter(|t| t.game_no == row.game_no).collect();
            let error_breakdown = game_summary.as_ref().and_then(|summary| {
                let complete = if saved_summary {
                    summary.total_games == 1
                        && summary.completed_games == 1
                        && covers_summary(summary, &game_totals)
                } else {
                    legacy_coverage
                        && game_totals
                            .iter()
                            .filter(|t| t.kind == "checker")
                            .map(|t| t.decisions)
                            .sum::<i32>()
                            == row.position_count
                };
                complete.then(|| typed_errors(&game_totals))
            });
            Ok(GameAnalysisStatistics {
                game_no: row.game_no,
                white_score: row.white_score,
                black_score: row.black_score,
                crawford: row.crawford,
                analyzed: row.analyzed,
                summary: game_summary,
                error_breakdown,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let checkpointed: Vec<_> = games.iter().filter(|game| game.analyzed).collect();
    let error_breakdown = summary.as_ref().and_then(|summary| {
        (summary.completed_games > 0
            && summary.completed_games as usize == checkpointed.len()
            && checkpointed
                .iter()
                .all(|game| game.error_breakdown.is_some())
            && covers_summary(summary, &all_totals))
        .then(|| typed_errors(&all_totals))
    });
    Ok(Json(JobAnalysisStatistics {
        job_id: id,
        summary,
        games,
        error_breakdown,
    }))
}

/// A saved summary is authoritative: incomplete decision storage must never
/// turn a missing typed count into an apparently valid zero.
fn covers_summary(summary: &MatchAnalysisSummary, totals: &[&DecisionTotals]) -> bool {
    if summary.players.iter().map(|p| p.player).collect::<Vec<_>>() != [0, 1] {
        return false;
    }
    let checker_count: i32 = totals
        .iter()
        .filter(|t| t.kind == "checker")
        .map(|t| t.decisions)
        .sum();
    summary.analyzed_moves == checker_count
        && summary.players.iter().all(|player| {
            [
                ("checker", player.checker_decisions),
                ("cube", player.cube_decisions),
            ]
            .into_iter()
            .all(|(kind, count)| {
                count >= 0
                    && totals
                        .iter()
                        .filter(|t| t.player == player.player && t.kind == kind)
                        .map(|t| t.decisions)
                        .sum::<i32>()
                        == count
            })
        })
}

fn typed_errors(totals: &[&DecisionTotals]) -> [PlayerErrorBreakdown; 2] {
    let mut players = [
        PlayerErrorBreakdown::default(),
        PlayerErrorBreakdown {
            player: 1,
            ..Default::default()
        },
    ];
    for total in totals {
        let player = &mut players[total.player as usize];
        let counts = if total.kind == "checker" {
            &mut player.checker
        } else {
            &mut player.cube
        };
        counts.doubtful += total.errors_doubtful;
        counts.bad += total.errors_bad;
        counts.very_bad += total.errors_very_bad;
    }
    players
}

#[derive(OpenApi)]
#[openapi(
    paths(statistics),
    components(schemas(
        JobAnalysisStatistics,
        GameAnalysisStatistics,
        PlayerErrorBreakdown,
        ErrorSeverityCounts
    ))
)]
pub struct StatisticsDoc;
