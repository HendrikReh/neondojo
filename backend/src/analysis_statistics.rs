pub mod catalog;
// Read-only statistics for exactly one native analysis job and its durable games.
use crate::json_errors::{Json, Path};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    configuration::ErrorThresholds,
    model::{Job, MatchAnalysisSummary, PlayerAnalysisSummary},
};
use axum::{Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection, Postgres, QueryBuilder};
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
    /// Historical checkpoint evaluated before NoCube rules were recorded.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub needs_cube_rule_reanalysis: bool,
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
    #[serde(default)]
    pub blunder: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct PlayerErrorBreakdown {
    pub player: i32,
    pub checker: ErrorSeverityCounts,
    pub cube: ErrorSeverityCounts,
}

#[derive(FromRow)]
struct GameRow {
    cube_enabled: Option<bool>,
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
    blunder: i32,
    loss: f64,
    errors_doubtful: i32,
    errors_bad: i32,
    errors_very_bad: i32,
    errors_blunder: i32,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(catalog::routes())
        .route("/api/jobs/{id}/statistics", get(statistics))
}

#[utoipa::path(get, path="/api/jobs/{id}/statistics", params(("id"=Uuid, Path)), responses((status=200, body=JobAnalysisStatistics), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Match analysis")]
async fn statistics(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<JobAnalysisStatistics>> {
    Ok(Json(load_statistics(&state, id).await?))
}

async fn load_verified_match_job(tx: &mut PgConnection, id: Uuid) -> ApiResult<Job> {
    let job: Job = sqlx::query_as("SELECT * FROM jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if job.kind != "match" {
        return Err(ApiError::invalid("This job is not a match analysis"));
    }
    let current_source: bool = sqlx::query_scalar(
        "SELECT j.source_revision=m.source_revision FROM jobs j JOIN matches m ON m.id=j.match_uuid WHERE j.id=$1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
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
    Ok(job)
}

async fn load_game_rows(tx: &mut PgConnection, id: Uuid, job: &Job) -> ApiResult<Vec<GameRow>> {
    let rows: Vec<GameRow> = sqlx::query_as(
        "SELECT g.game_no,g.white_score,g.black_score,g.crawford,g.cube_enabled,g.position_count,c.job_id IS NOT NULL AS analyzed,c.summary FROM match_games g LEFT JOIN match_analysis_games c ON c.job_id=$1 AND c.game_no=g.game_no WHERE g.match_uuid=$2 AND ($3::integer IS NULL OR g.game_no=$3) ORDER BY g.game_no",
    )
    .bind(id)
    .bind(job.match_uuid)
    .bind(job.game_no)
    .fetch_all(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    if !rows.is_empty() {
        return Ok(rows);
    }
    // Pre-catalog libraries still have immutable SGF and durable checkpoints.
    // Recover the source metadata in memory, keeping this endpoint read-only.
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
    Ok(games
        .into_iter()
        .filter(|g| job.game_no.is_none_or(|n| n == g.game_no))
        .map(|g| {
            let checkpoint = checkpoints.iter().find(|(n, _)| *n == g.game_no);
            GameRow {
                cube_enabled: g.cube_enabled,
                game_no: g.game_no,
                white_score: g.white_score,
                black_score: g.black_score,
                crawford: g.crawford,
                position_count: g.position_count,
                analyzed: checkpoint.is_some(),
                summary: checkpoint.and_then(|(_, s)| s.clone()),
            }
        })
        .collect())
}

async fn load_decision_totals(
    tx: &mut PgConnection,
    id: Uuid,
    game_no: Option<i32>,
    thresholds: ErrorThresholds,
) -> ApiResult<Vec<DecisionTotals>> {
    let mut decision_totals = QueryBuilder::<Postgres>::new(
        "SELECT game_no,player,kind,count(*)::integer AS decisions,
            count(*) FILTER(WHERE status='forced')::integer AS forced,
            count(*) FILTER(WHERE status='unavailable')::integer AS unavailable,
            sum(equity_loss) AS loss",
    );
    for (grade, _) in thresholds.bands() {
        let name = grade.as_str();
        decision_totals
            .push(",count(*) FILTER(WHERE saved_grade=")
            .push_bind(name)
            .push(")::integer AS ")
            .push(name)
            .push(",count(*) FILTER(WHERE grade=")
            .push_bind(name)
            .push(")::integer AS errors_")
            .push(name);
    }
    decision_totals.push(
        " FROM (SELECT m.game_no,m.player,m.kind,m.equity_loss,
        m.data->>'evaluation_status' AS status,m.data->>'severity' AS saved_grade,",
    );
    thresholds.push_classification(
        &mut decision_totals,
        "m.equity_loss",
        "m.data->>'evaluation_status'",
    );
    decision_totals
        .push(
            " AS grade FROM move_analyses m JOIN match_analysis_games c
        ON c.job_id=m.job_id AND c.game_no=m.game_no WHERE m.job_id=",
        )
        .push_bind(id);
    if let Some(game_no) = game_no {
        decision_totals.push(" AND m.game_no=").push_bind(game_no);
    }
    decision_totals.push(") AS moves GROUP BY game_no,player,kind");
    Ok(decision_totals
        .build_query_as()
        .fetch_all(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?)
}

fn legacy_game_summary(
    row: &GameRow,
    plies: i32,
    summary: Option<&MatchAnalysisSummary>,
    totals: &[&DecisionTotals],
) -> MatchAnalysisSummary {
    let mut legacy = MatchAnalysisSummary {
        engine: summary.map_or_else(
            || "GNUbg (version unavailable)".into(),
            |s| s.engine.clone(),
        ),
        plies,
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
    for total in totals {
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
        player.blunder += total.blunder;
    }
    legacy
}

fn build_game_statistics(
    row: GameRow,
    plies: i32,
    summary: Option<&MatchAnalysisSummary>,
    totals: &[DecisionTotals],
    legacy_coverage: bool,
) -> anyhow::Result<GameAnalysisStatistics> {
    let needs_cube_rule_reanalysis = row.analyzed
        && row.cube_enabled == Some(false)
        && row
            .summary
            .as_ref()
            .and_then(|s| s["cube_enabled"].as_bool())
            != Some(false);
    let saved_summary = row.summary.is_some();
    let game_totals: Vec<_> = totals.iter().filter(|t| t.game_no == row.game_no).collect();
    let game_summary = if !row.analyzed {
        None
    } else if let Some(saved) = row.summary {
        Some(serde_json::from_value(saved)?)
    } else {
        Some(legacy_game_summary(&row, plies, summary, &game_totals))
    };
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
        needs_cube_rule_reanalysis,
        game_no: row.game_no,
        white_score: row.white_score,
        black_score: row.black_score,
        crawford: row.crawford,
        analyzed: row.analyzed,
        summary: game_summary,
        error_breakdown,
    })
}

pub(crate) async fn load_statistics(
    state: &AppState,
    id: Uuid,
) -> ApiResult<JobAnalysisStatistics> {
    // Complete the source visibility check before holding a pool connection.
    let visibility_job = crate::store::get_job(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    crate::play::require_job_review_available(&state.pool, &visibility_job).await?;
    // A worker may checkpoint while this request runs. All summary and coverage
    // reads must describe the same committed database snapshot.
    let mut tx = crate::database_pool::begin_read_snapshot(&state.pool).await?;
    let job = load_verified_match_job(&mut tx, id).await?;
    let rows = load_game_rows(&mut tx, id, &job).await?;
    let thresholds = crate::configuration::store::error_thresholds(&mut *tx).await?;
    let totals = load_decision_totals(&mut tx, id, job.game_no, thresholds).await?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    let summary: Option<MatchAnalysisSummary> = job
        .result
        .map(serde_json::from_value)
        .transpose()
        .map_err(anyhow::Error::from)?;
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
            build_game_statistics(row, job.plies, summary.as_ref(), &totals, legacy_coverage)
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
    Ok(JobAnalysisStatistics {
        job_id: id,
        summary,
        games,
        error_breakdown,
    })
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
        counts.blunder += total.errors_blunder;
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
