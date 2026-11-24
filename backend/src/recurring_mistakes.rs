//! Read-only questions and aggregates from native and immutable imported analyses.
use crate::json_errors::{Json, Query};
use crate::player_scope;
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    game_plan::GamePlan,
    match_analysis::{Decision, cube_analysis_available},
    model::Position,
    practice::{
        cube_context,
        evaluation::{self, PracticeAnswer, PracticeEvaluation, SavedEvaluation},
    },
};
use axum::{
    Router,
    extract::State,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashSet;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

mod history;
mod imported;
pub(crate) mod imported_projection;
mod index;
#[doc(hidden)]
pub mod query_plans;
mod search;
pub(crate) mod source;
use source::{SourceDecision, load, players};

const MIN_RANKED_DECISIONS: usize = 30;
const MAX_PRACTICE_QUESTIONS: usize = 20;

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
    /// Count only matches played without learning aids: imports and bot
    /// matches in Competition mode without a hint.
    pub independent_only: Option<bool>,
    /// A match name, such as the configured profile name, to translate into
    /// the player choice that covers it.
    pub resolve: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct MistakeGroup {
    pub game_plan: GamePlan,
    pub mistake_kind: MistakeKind,
    pub count: usize,
    pub match_count: usize,
    pub total_loss: f64,
    /// Null when the decision type has no defined set of opportunities.
    #[schema(required = true)]
    pub rates: Option<MistakeRates>,
}
#[derive(Serialize, ToSchema)]
pub struct MistakeRates {
    /// Evaluable decisions in the same game plan and decision category,
    /// independent of the error threshold. Cube categories use the best action.
    pub decision_count: usize,
    /// Fraction of those decisions that meet the positive loss threshold.
    pub error_rate: f64,
    /// Sum of qualifying normalized equity losses per 100 evaluable decisions.
    pub loss_per_100: f64,
    /// True when there are fewer than 30 evaluable decisions.
    pub small_sample: bool,
    /// Error rate in the earlier and the recent half of the player's matches.
    /// Null unless both halves hold at least 10 evaluable decisions.
    #[schema(required = true)]
    pub trend: Option<MistakeTrend>,
}
#[derive(Serialize, ToSchema)]
pub struct MistakeTrend {
    pub earlier_error_rate: f64,
    pub recent_error_rate: f64,
    pub earlier_decisions: usize,
    pub recent_decisions: usize,
}
#[derive(Serialize, ToSchema)]
pub struct Mistakes {
    pub players: Vec<String>,
    pub groups: Vec<MistakeGroup>,
    pub analyzed_decisions: usize,
    pub analyzed_matches: usize,
    /// The entry of `players` that stands for "me": the virtual profile
    /// chosen as My player profile, or else the entry that covers the name
    /// given as `resolve`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_player: Option<String>,
    /// All matches of the chosen player in the scope, under any alias.
    pub player_matches: usize,
    /// Those of them without any analyzed decision, whatever the game plan.
    pub unanalyzed_matches: usize,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct MistakePracticeRequest {
    pub player: String,
    pub database_id: Option<Uuid>,
    pub game_plan: GamePlan,
    pub min_loss: Option<f64>,
    pub mistake_kind: MistakeKind,
    /// Ask only from matches played without learning aids.
    #[serde(default)]
    pub independent_only: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, ToSchema)]
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
    Sgf,
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

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct MistakeAttemptRequest {
    reference: MistakeReference,
    answer: PracticeAnswer,
}

#[derive(Serialize, ToSchema)]
struct MistakeAttempt {
    evaluation: PracticeEvaluation,
    original: MistakeAnswer,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/study/mistakes", get(list))
        .route("/api/study/decisions", get(search::search))
        .route("/api/study/mistakes/history", get(history::list))
        .route("/api/study/mistakes/practice", post(practice))
        .route("/api/study/mistakes/reveal", post(reveal))
        .route("/api/study/mistakes/attempt", post(attempt))
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
fn opportunity(d: &Decision) -> Option<MistakeKind> {
    match (d.kind.as_str(), d.played.as_str(), d.best.as_str()) {
        ("checker", _, _) => Some(MistakeKind::Checker),
        ("cube", "no double" | "double", "double") => Some(MistakeKind::MissedDouble),
        ("cube", "no double" | "double", "no double") => Some(MistakeKind::PrematureDouble),
        ("cube", "take" | "drop", "drop") => Some(MistakeKind::WrongTake),
        ("cube", "take" | "drop", "take") => Some(MistakeKind::WrongPass),
        _ => None,
    }
}

pub(crate) fn kind(d: &Decision) -> MistakeKind {
    if d.kind == "checker" || d.played != d.best {
        opportunity(d).unwrap_or(MistakeKind::UnclassifiedCube)
    } else {
        MistakeKind::UnclassifiedCube
    }
}

fn correct_cube_opportunity(d: &Decision) -> Option<MistakeKind> {
    if d.kind == "cube" && d.played == d.best && d.equity_loss == 0.0 {
        opportunity(d)
    } else {
        None
    }
}

fn evaluation_position(question: &MistakePracticeQuestion) -> ApiResult<Cow<'_, Position>> {
    if matches!(question.source, Some(MistakeSource::Native)) && question.kind == "cube_response" {
        let mut position = question.position.clone();
        position.gnubg_match_id =
            crate::import::ids::without_offer_marker(&position.gnubg_match_id)
                .ok_or_else(|| ApiError::invalid("This question has no valid cube context"))?;
        Ok(Cow::Owned(position))
    } else {
        Ok(Cow::Borrowed(&question.position))
    }
}

fn can_practice_correct_cube(row: &SourceDecision) -> bool {
    match correct_cube_opportunity(&row.decision) {
        Some(MistakeKind::PrematureDouble) => {
            let p = &row.question.position;
            cube_context(p, false).is_ok()
                && cube_analysis_available(&p.board, &p.gnubg_match_id, p.player).unwrap_or(false)
        }
        Some(MistakeKind::WrongTake | MistakeKind::WrongPass) => {
            evaluation_position(&row.question).is_ok_and(|p| cube_context(&p, true).is_ok())
        }
        _ => false,
    }
}

#[utoipa::path(get,path="/api/study/mistakes",params(MistakesQuery),responses((status=200,body=Mistakes),(status=422,body=ErrorBody)),tag="Study")]
async fn list(
    State(s): State<AppState>,
    Query(q): Query<MistakesQuery>,
) -> ApiResult<Json<Mistakes>> {
    let (player, loss) = validate(q.player, q.min_loss)?;
    let players = players(&s.pool, q.database_id).await?;
    let mut result = Mistakes {
        players,
        groups: vec![],
        analyzed_decisions: 0,
        analyzed_matches: 0,
        player_matches: 0,
        unanalyzed_matches: 0,
        resolved_player: None,
    };
    let configured = validate(q.resolve, None)?.0;
    if let Some(shown) = player_scope::my_choice(&s.pool, configured).await? {
        result.resolved_player = result
            .players
            .iter()
            .find(|player| player.to_lowercase() == shown.to_lowercase())
            .cloned();
    }
    let Some(player) = player else {
        return Ok(Json(result));
    };
    let names = player_scope::names(&s.pool, &player).await?;
    let independent_only = q.independent_only.unwrap_or(false);
    let (groups, decisions, matches) = index::ensure_and_group(
        &s.pool,
        q.database_id,
        &names,
        q.game_plan,
        loss,
        independent_only,
    )
    .await?;
    result.groups = groups;
    result.analyzed_decisions = decisions;
    result.analyzed_matches = matches;
    (result.player_matches, result.unanalyzed_matches) =
        index::coverage(&s.pool, q.database_id, &names, independent_only).await?;
    Ok(Json(result))
}
#[utoipa::path(post,path="/api/study/mistakes/practice",request_body=MistakePracticeRequest,responses((status=200,body=MistakePractice),(status=422,body=ErrorBody)),tag="Study")]
async fn practice(
    State(s): State<AppState>,
    Json(q): Json<MistakePracticeRequest>,
) -> ApiResult<Json<MistakePractice>> {
    let (player, loss) = validate(Some(q.player), q.min_loss)?;
    let player = player.unwrap();
    let names = player_scope::names(&s.pool, &player).await?;
    let mut rows = load(&s.pool, q.database_id, None, Some(&names)).await?;
    if q.independent_only {
        let matches: Vec<Uuid> = rows
            .iter()
            .map(|row| row.question.position.match_uuid)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let independent = index::independent_matches(&s.pool, &matches).await?;
        rows.retain(|row| independent.contains(&row.question.position.match_uuid));
    }
    let items = select_practice_questions(rows, &names, q.game_plan, q.mistake_kind, loss);
    Ok(Json(MistakePractice { items }))
}

fn select_practice_questions(
    mut rows: Vec<SourceDecision>,
    player: &[String],
    game_plan: GamePlan,
    mistake_kind: MistakeKind,
    min_loss: f64,
) -> Vec<MistakePracticeQuestion> {
    let contrast_opportunity = match mistake_kind {
        MistakeKind::MissedDouble => Some(MistakeKind::PrematureDouble),
        MistakeKind::WrongTake => Some(MistakeKind::WrongPass),
        MistakeKind::WrongPass => Some(MistakeKind::WrongTake),
        _ => None,
    };
    rows.retain(|r| {
        r.in_scope(player, Some(game_plan))
            && ((r.meets_loss_threshold(min_loss) && kind(&r.decision) == mistake_kind)
                || contrast_opportunity.is_some_and(|contrast| {
                    correct_cube_opportunity(&r.decision) == Some(contrast)
                }))
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
    let mistake_count = rows.partition_point(|r| r.decision.equity_loss > 0.0);
    let mut seen = HashSet::new();
    let mut unique = |q: &MistakePracticeQuestion| {
        seen.insert((
            q.position.position_id.clone(),
            q.position.gnubg_match_id.clone(),
            q.kind.clone(),
        ))
    };
    let mut items: Vec<_> = rows
        .drain(..mistake_count)
        .map(|r| r.question)
        .filter(&mut unique)
        .take(MAX_PRACTICE_QUESTIONS)
        .collect();
    if contrast_opportunity.is_some() && !items.is_empty() {
        let contrasts: Vec<_> = rows
            .into_iter()
            .filter(can_practice_correct_cube)
            .map(|r| r.question)
            .filter(unique)
            .take(items.len().min(MAX_PRACTICE_QUESTIONS / 2))
            .collect();
        items.truncate(MAX_PRACTICE_QUESTIONS - contrasts.len());
        items.extend(contrasts);
        items.sort_by_cached_key(|_| Uuid::new_v4());
    }
    items
}
#[utoipa::path(post,path="/api/study/mistakes/reveal",request_body=MistakeReference,responses((status=200,body=MistakeAnswer),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Study")]
async fn reveal(
    State(s): State<AppState>,
    Json(reference): Json<MistakeReference>,
) -> ApiResult<Json<MistakeAnswer>> {
    Ok(Json(original_answer(load_question(&s, &reference).await?)))
}

#[utoipa::path(post,path="/api/study/mistakes/attempt",request_body=MistakeAttemptRequest,responses((status=200,body=MistakeAttempt),(status=404,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Study")]
async fn attempt(
    State(s): State<AppState>,
    Json(input): Json<MistakeAttemptRequest>,
) -> ApiResult<Json<MistakeAttempt>> {
    let row = load_question(&s, &input.reference).await?;
    let native = matches!(row.question.source, Some(MistakeSource::Native));
    let position = evaluation_position(&row.question)?;
    let evaluation = evaluation::evaluate(
        &s,
        &row.question.kind,
        &position,
        SavedEvaluation {
            engine: &row.question.engine,
            plies: row.question.plies,
            decision: Some(&row.decision),
            candidates: &[],
            native,
        },
        &input.answer,
    )
    .await?;
    // An engine run can outlive a change in the winning source or competition state.
    if evaluation.is_recomputed() {
        load_question(&s, &input.reference).await?;
    }
    history::record(&s.pool, &row, &evaluation).await?;
    Ok(Json(MistakeAttempt {
        evaluation,
        original: original_answer(row),
    }))
}

async fn load_question(s: &AppState, reference: &MistakeReference) -> ApiResult<SourceDecision> {
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
    load(&s.pool, None, Some(match_id), None)
        .await?
        .into_iter()
        .find(|r| {
            &r.question.reference == reference
                && (r.decision.equity_loss > 0.0 || can_practice_correct_cube(r))
        })
        .ok_or_else(ApiError::missing)
}

fn original_answer(row: SourceDecision) -> MistakeAnswer {
    MistakeAnswer {
        best: row.decision.best,
        played: row.decision.played,
        equity_loss: row.decision.equity_loss,
        engine: row.question.engine,
        plies: row.question.plies,
        depth: row.question.depth,
        method: row.question.method,
        source: row.question.source,
    }
}
#[derive(OpenApi)]
#[openapi(
    paths(list, practice, reveal, attempt, search::search, history::list),
    components(schemas(
        MistakeKind,
        MistakeSource,
        MistakeGroup,
        MistakeRates,
        MistakeTrend,
        Mistakes,
        MistakePracticeRequest,
        MistakeReference,
        MistakePracticeQuestion,
        MistakePractice,
        MistakeAnswer,
        MistakeAttemptRequest,
        MistakeAttempt,
        search::DecisionKind,
        search::CubeOwnerFilter,
        search::LossDirection,
        search::DecisionHit,
        search::DecisionSearchPage,
        history::MistakePracticeDay,
        history::MistakePracticeHistory
    ))
)]
pub struct RecurringMistakesDoc;
