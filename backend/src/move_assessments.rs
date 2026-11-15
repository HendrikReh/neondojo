//! Checker marks use the same selected, validated decisions as review and statistics.
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::Analysis,
    match_analysis::Decision,
    model::{MoveSeverity, Position},
    review_decisions::load_all_review,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Debug, Deserialize, IntoParams)]
pub struct MoveAssessmentQuery {
    pub game_no: i32,
    /// Legacy explicit job identity; use analysis_id for native and imported results.
    pub job_id: Option<Uuid>,
    pub analysis_id: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentSource {
    Native,
    Imported,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MoveAssessment {
    pub position_id: Uuid,
    pub severity: MoveSeverity,
    pub equity_loss: f64,
    pub best_move: Option<String>,
    pub source: AssessmentSource,
    pub engine: String,
    pub depth: Option<String>,
    pub depth_plies: Option<i32>,
    pub job_id: Option<Uuid>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct GameMoveAssessments {
    pub match_id: Uuid,
    pub game_no: i32,
    pub job_id: Option<Uuid>,
    pub analysis_id: Option<String>,
    pub items: Vec<MoveAssessment>,
}
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/matches/{id}/move-assessments", get(assessments))
}
#[utoipa::path(get,path="/api/matches/{id}/move-assessments",params(("id"=Uuid,Path),MoveAssessmentQuery),responses((status=200,body=GameMoveAssessments),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Match analysis")]
async fn assessments(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<MoveAssessmentQuery>, axum::extract::rejection::QueryRejection>,
) -> ApiResult<Json<GameMoveAssessments>> {
    let Query(query) = query.map_err(|e| ApiError::invalid(e.body_text()))?;
    if query.game_no <= 0 {
        return Err(ApiError::invalid("game_no must be positive"));
    }
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
    let selected = crate::analysis_catalog::resolve_selection(
        &state.pool,
        id,
        query.analysis_id.as_deref().or(legacy.as_deref()),
    )
    .await?;
    if query.job_id.is_some()
        && let Some(crate::analysis_catalog::SelectedAnalysis::Job(job)) = &selected
        && job.game_no.is_some_and(|game| game != query.game_no)
    {
        return Err(ApiError::invalid("Job belongs to another game"));
    }
    let analysis_id = selected.as_ref().map(|s| s.id());
    let mut items = Vec::new();
    if let Some(ref key) = analysis_id {
        let rows =
            load_all_review(&state, id, key, Some(query.game_no), Some("checker".into())).await?;
        items.extend(rows.into_iter().filter_map(|r| {
            if !matches!(
                r.decision.evaluation_status.as_str(),
                "evaluated" | "forced"
            ) {
                return None;
            }
            Some(MoveAssessment {
                position_id: r.position_uuid?,
                severity: serde_json::from_value(Value::String(r.decision.severity)).ok()?,
                equity_loss: r.decision.equity_loss,
                best_move: Some(r.decision.best),
                source: r.source,
                engine: r.engine,
                depth: r.depth,
                depth_plies: r.depth_plies,
                job_id: r.job_id,
            })
        }));
    } else {
        // Still validate nonexistent games for an empty catalog.
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM match_games WHERE match_uuid=$1 AND game_no=$2)",
        )
        .bind(id)
        .bind(query.game_no)
        .fetch_one(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
        if !exists {
            return Err(ApiError::missing());
        }
    }
    Ok(Json(GameMoveAssessments {
        match_id: id,
        game_no: query.game_no,
        job_id: query.job_id,
        analysis_id,
        items,
    }))
}

pub(crate) fn rounded_loss(played: f64, best: f64) -> Option<f64> {
    let loss = ((best - played) * 1_000_000.0).round() / 1_000_000.0;
    (played.is_finite() && best.is_finite() && loss.is_finite() && loss >= 0.0).then_some(loss)
}

pub(crate) fn native_checker_loss(
    position: &Position,
    data: &Value,
    equity_loss: f64,
) -> Option<f64> {
    if data["kind"] != "checker"
        || data["player"].as_i64()? != i64::from(position.player)
        || data["best"].as_str()?.trim().is_empty()
        || data["played"].as_str()?.trim().is_empty()
        || !equity_loss.is_finite()
        || equity_loss < 0.0
    {
        return None;
    }
    let actual = after_play(position, &position.notation)?;
    if after_play(position, data["played"].as_str()?)? != actual {
        return None;
    }
    after_play(position, data["best"].as_str()?)?;
    let stored = data["equity_loss"].as_f64()?;
    if !stored.is_finite() || (stored - equity_loss).abs() > 1e-9 {
        return None;
    }
    let loss = match data["evaluation_status"].as_str()? {
        "forced" if stored == 0.0 && after_play(position, "Cannot move").is_some() => 0.0,
        "evaluated" => rounded_loss(
            data["played_equity"].as_f64()?,
            data["best_equity"].as_f64()?,
        )?,
        _ => return None,
    };
    ((loss - stored).abs() <= 1e-9).then_some(loss)
}

/// A stored single-position candidate list can grade the played move only when
/// that exact legal final board is included. Missing candidates stay unassessed.
pub(crate) fn hint_decision(position: &Position, analysis: &Analysis) -> Option<Decision> {
    if analysis.engine.trim().is_empty() || analysis.candidates.len() > 256 {
        return None;
    }
    let best = analysis.candidates.iter().find(|c| c.rank == 1)?;
    let actual = after_play(position, &position.notation)?;
    after_play(position, &best.notation)?;
    let played = analysis
        .candidates
        .iter()
        .find(|c| after_play(position, &c.notation) == Some(actual))?;
    rounded_loss(played.equity, best.equity)?;
    let forced = after_play(position, "Cannot move").is_some();
    let mut decision = crate::match_analysis::decision(
        position.player,
        "checker",
        &played.notation,
        &best.notation,
        (!forced).then_some((played.equity, best.equity)),
    );
    decision.decision_no = position.ply;
    decision.checker_no = Some(position.ply);
    decision.dice = position.dice.as_slice().try_into().ok();
    Some(decision)
}

/// Resolve reordered, grouped and combined checker notation to the same legal
/// final board. Mere absence of the played move in a top-N hint stays unknown.
pub(crate) fn after_play(position: &Position, notation: &str) -> Option<[i32; 26]> {
    after_checker_play(
        position.board.as_slice().try_into().ok()?,
        position.player,
        position.dice.as_slice().try_into().ok()?,
        notation,
    )
}

pub(crate) fn after_checker_play(
    mut board: [i32; 26],
    player: i32,
    dice: [i32; 2],
    notation: &str,
) -> Option<[i32; 26]> {
    if notation.trim().is_empty() || notation.len() > 160 {
        return None;
    }
    let mut runs = Vec::new();
    if !matches!(
        notation.trim().to_lowercase().as_str(),
        "cannot move" | "can't move" | "dance"
    ) {
        for token in notation.split_whitespace() {
            let (path, count) = if let Some((path, count)) =
                token.strip_suffix(')').and_then(|s| s.rsplit_once('('))
            {
                let count: usize = count.parse().ok()?;
                if !(1..=4).contains(&count) {
                    return None;
                }
                (path, count)
            } else {
                (token, 1)
            };
            let points: Option<Vec<usize>> = path
                .split('/')
                .map(|point| {
                    let point = point.strip_suffix('*').unwrap_or(point);
                    match point.to_lowercase().as_str() {
                        "bar" => Some(25),
                        "off" => Some(0),
                        number => number
                            .parse::<usize>()
                            .ok()
                            .filter(|p| (1..=24).contains(p)),
                    }
                })
                .collect();
            let points = points?;
            if points.len() < 2 {
                return None;
            }
            for _ in 0..count {
                for pair in points.windows(2) {
                    runs.push((pair[0], pair[1]));
                }
            }
            if runs.len() > 4 {
                return None;
            }
        }
    }
    crate::import::sgf::expand_xg_play(&mut board, player, dice, &runs).ok()?;
    Some(board)
}

#[derive(OpenApi)]
#[openapi(
    paths(assessments),
    components(schemas(GameMoveAssessments, MoveAssessment, AssessmentSource))
)]
pub struct MoveAssessmentsDoc;
