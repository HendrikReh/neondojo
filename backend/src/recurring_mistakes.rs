//! Read-only questions and aggregates from native and immutable imported analyses.
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    game_plan::GamePlan,
    match_analysis::Decision,
    model::Position,
};
use axum::{
    Json, Router,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

mod imported;
pub(crate) mod imported_projection;
pub(crate) mod source;
use source::{load, players};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MistakeKind {
    Checker,
    MissedDouble,
    PrematureDouble,
    WrongTake,
    WrongPass,
    UnclassifiedCube,
}
#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in=Query)]
pub struct MistakesQuery {
    pub player: Option<String>,
    pub database_id: Option<Uuid>,
    pub game_plan: Option<GamePlan>,
    pub min_loss: Option<f64>,
}
#[derive(Serialize, ToSchema)]
pub struct MistakeGroup {
    pub game_plan: GamePlan,
    pub mistake_kind: MistakeKind,
    pub count: usize,
    pub match_count: usize,
    pub total_loss: f64,
}
#[derive(Serialize, ToSchema)]
pub struct Mistakes {
    pub players: Vec<String>,
    pub groups: Vec<MistakeGroup>,
    pub analyzed_decisions: usize,
    pub analyzed_matches: usize,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MistakePracticeRequest {
    pub player: String,
    pub database_id: Option<Uuid>,
    pub game_plan: GamePlan,
    pub min_loss: Option<f64>,
    pub mistake_kind: MistakeKind,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MistakeReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imported_analysis_id: Option<Uuid>,
    pub game_no: i32,
    pub decision_no: i32,
}
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MistakeSource {
    Native,
    Xg,
    Bgblitz,
}

#[derive(Serialize, ToSchema)]
pub struct MistakePracticeQuestion {
    pub reference: MistakeReference,
    /// checker, cube_offer or cube_response.
    pub kind: String,
    pub position: Position,
    pub players: [String; 2],
    pub engine: String,
    pub plies: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<MistakeSource>,
}
#[derive(Serialize, ToSchema)]
pub struct MistakePractice {
    pub items: Vec<MistakePracticeQuestion>,
}
#[derive(Serialize, ToSchema)]
pub struct MistakeAnswer {
    pub best: String,
    pub played: String,
    pub equity_loss: f64,
    pub engine: String,
    pub plies: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<MistakeSource>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/study/mistakes", get(list))
        .route("/api/study/mistakes/practice", post(practice))
        .route("/api/study/mistakes/reveal", post(reveal))
}
fn validate(player: Option<String>, loss: Option<f64>) -> ApiResult<(Option<String>, f64)> {
    let player = player.map(|p| p.trim().to_owned());
    if player
        .as_ref()
        .is_some_and(|p| p.is_empty() || p.chars().count() > 240 || p.chars().any(char::is_control))
    {
        return Err(ApiError::invalid(
            "Choose a nonempty exact player name of at most 240 characters",
        ));
    }
    let loss = loss.unwrap_or(0.08);
    if !loss.is_finite() || !(0.0..=1000.0).contains(&loss) {
        return Err(ApiError::invalid(
            "min_loss must be finite and between 0 and 1000",
        ));
    }
    Ok((player, loss))
}
fn kind(d: &Decision) -> MistakeKind {
    match (d.kind.as_str(), d.played.as_str(), d.best.as_str()) {
        ("checker", _, _) => MistakeKind::Checker,
        ("cube", "no double", "double") => MistakeKind::MissedDouble,
        ("cube", "double", "no double") => MistakeKind::PrematureDouble,
        ("cube", "take", "drop") => MistakeKind::WrongTake,
        ("cube", "drop", "take") => MistakeKind::WrongPass,
        _ => MistakeKind::UnclassifiedCube,
    }
}
#[utoipa::path(get,path="/api/study/mistakes",params(MistakesQuery),responses((status=200,body=Mistakes),(status=422,body=ErrorBody)),tag="Study")]
async fn list(
    State(s): State<AppState>,
    input: Result<Query<MistakesQuery>, QueryRejection>,
) -> ApiResult<Json<Mistakes>> {
    let q = input.map_err(|e| ApiError::invalid(e.body_text()))?.0;
    let (player, loss) = validate(q.player, q.min_loss)?;
    let players = players(&s.pool, q.database_id).await?;
    let mut result = Mistakes {
        players,
        groups: vec![],
        analyzed_decisions: 0,
        analyzed_matches: 0,
    };
    let Some(player) = player else {
        return Ok(Json(result));
    };
    let rows = load(&s.pool, q.database_id, None, Some(&player)).await?;
    let mut coverage = HashSet::new();
    let mut groups =
        BTreeMap::<(String, MistakeKind), (GamePlan, usize, BTreeSet<Uuid>, f64)>::new();
    for row in rows {
        if row.question.players[row.decision.player as usize]
            .trim()
            .to_lowercase()
            != player.to_lowercase()
            || q.game_plan.is_some_and(|p| p != row.plan)
        {
            continue;
        }
        result.analyzed_decisions += 1;
        coverage.insert(row.question.position.match_uuid);
        if !row.matches(&player, q.game_plan, loss) {
            continue;
        }
        let label = crate::position_game_plans::label(row.plan).to_owned();
        let group = groups.entry((label, kind(&row.decision))).or_insert((
            row.plan,
            0,
            BTreeSet::new(),
            0.0,
        ));
        group.1 += 1;
        group.2.insert(row.question.position.match_uuid);
        group.3 += row.decision.equity_loss;
    }
    result.analyzed_matches = coverage.len();
    result.groups = groups
        .into_iter()
        .map(
            |((_, mistake_kind), (game_plan, count, matches, total_loss))| MistakeGroup {
                game_plan,
                mistake_kind,
                count,
                match_count: matches.len(),
                total_loss,
            },
        )
        .collect();
    result
        .groups
        .sort_by(|a, b| b.total_loss.total_cmp(&a.total_loss));
    Ok(Json(result))
}
#[utoipa::path(post,path="/api/study/mistakes/practice",request_body=MistakePracticeRequest,responses((status=200,body=MistakePractice),(status=422,body=ErrorBody)),tag="Study")]
async fn practice(
    State(s): State<AppState>,
    input: Result<Json<MistakePracticeRequest>, JsonRejection>,
) -> ApiResult<Json<MistakePractice>> {
    let q = input.map_err(|e| ApiError::invalid(e.body_text()))?.0;
    let (player, loss) = validate(Some(q.player), q.min_loss)?;
    let player = player.unwrap();
    let mut rows = load(&s.pool, q.database_id, None, Some(&player)).await?;
    rows.retain(|r| {
        r.matches(&player, Some(q.game_plan), loss) && kind(&r.decision) == q.mistake_kind
    });
    rows.sort_by(|a, b| {
        b.decision
            .equity_loss
            .total_cmp(&a.decision.equity_loss)
            .then_with(|| {
                a.question
                    .reference
                    .job_id
                    .cmp(&b.question.reference.job_id)
            })
            .then_with(|| {
                a.question
                    .reference
                    .imported_analysis_id
                    .cmp(&b.question.reference.imported_analysis_id)
            })
            .then_with(|| {
                a.question
                    .reference
                    .game_no
                    .cmp(&b.question.reference.game_no)
            })
            .then_with(|| {
                a.question
                    .reference
                    .decision_no
                    .cmp(&b.question.reference.decision_no)
            })
    });
    let mut seen = HashSet::new();
    let items = rows
        .into_iter()
        .filter(|r| {
            seen.insert((
                r.question.position.position_id.clone(),
                r.question.position.gnubg_match_id.clone(),
                r.question.kind.clone(),
            ))
        })
        .take(20)
        .map(|r| r.question)
        .collect();
    Ok(Json(MistakePractice { items }))
}
#[utoipa::path(post,path="/api/study/mistakes/reveal",request_body=MistakeReference,responses((status=200,body=MistakeAnswer),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Study")]
async fn reveal(
    State(s): State<AppState>,
    input: Result<Json<MistakeReference>, JsonRejection>,
) -> ApiResult<Json<MistakeAnswer>> {
    let reference = input.map_err(|e| ApiError::invalid(e.body_text()))?.0;
    if reference.job_id.is_some() == reference.imported_analysis_id.is_some() {
        return Err(ApiError::invalid(
            "Supply exactly one of job_id or imported_analysis_id",
        ));
    }
    if reference.game_no < 1 || reference.decision_no < 1 {
        return Err(ApiError::invalid(
            "game_no and decision_no must be positive",
        ));
    }
    let match_id: Option<Uuid> = if let Some(job_id) = reference.job_id {
        sqlx::query_scalar("SELECT match_uuid FROM jobs WHERE id=$1 AND kind='match'")
            .bind(job_id)
            .fetch_optional(&s.pool)
            .await
            .map_err(anyhow::Error::from)?
            .flatten()
    } else {
        sqlx::query_scalar("SELECT match_uuid FROM imported_analyses WHERE id=$1")
            .bind(reference.imported_analysis_id)
            .fetch_optional(&s.pool)
            .await
            .map_err(anyhow::Error::from)?
    };
    let match_id = match_id.ok_or_else(ApiError::missing)?;
    let row = load(&s.pool, None, Some(match_id), None)
        .await?
        .into_iter()
        .find(|r| r.question.reference == reference && r.decision.equity_loss > 0.0)
        .ok_or_else(ApiError::missing)?;
    Ok(Json(MistakeAnswer {
        best: row.decision.best,
        played: row.decision.played,
        equity_loss: row.decision.equity_loss,
        engine: row.question.engine,
        plies: row.question.plies,
        depth: row.question.depth,
        method: row.question.method,
        source: row.question.source,
    }))
}
#[derive(OpenApi)]
#[openapi(
    paths(list, practice, reveal),
    components(schemas(
        MistakeKind,
        MistakeSource,
        MistakeGroup,
        Mistakes,
        MistakePracticeRequest,
        MistakeReference,
        MistakePracticeQuestion,
        MistakePractice,
        MistakeAnswer
    ))
)]
pub struct RecurringMistakesDoc;
