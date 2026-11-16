//! Filtered, paginated list of one player's analyzed decisions.
//!
//! The list reads the same selected analyses as the mistake groups, so a
//! decision found here can be practiced through the same references.
use super::{
    MistakeKind, MistakePracticeQuestion, kind as mistake_kind, source::SourceDecision, validate,
};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    cube_context::cube_context,
    game_plan::GamePlan,
};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 100;
const MAX_PAGE: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Checker,
    CubeOffer,
    CubeResponse,
}

impl DecisionKind {
    fn name(self) -> &'static str {
        match self {
            Self::Checker => "checker",
            Self::CubeOffer => "cube_offer",
            Self::CubeResponse => "cube_response",
        }
    }
}

#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
pub struct DecisionSearchQuery {
    /// Exact player name, ignoring case and surrounding spaces.
    pub player: String,
    pub database_id: Option<Uuid>,
    pub kind: Option<DecisionKind>,
    /// Only errors of this type; implies a positive loss.
    pub mistake_kind: Option<MistakeKind>,
    pub game_plan: Option<GamePlan>,
    /// Minimum normalized equity loss; 0 includes correct decisions.
    pub min_loss: Option<f64>,
    pub max_loss: Option<f64>,
    /// Points the deciding player still needs; match play only.
    pub away: Option<i32>,
    /// Points the opponent still needs; match play only.
    pub opponent_away: Option<i32>,
    /// Cube value before the decision.
    pub cube_value: Option<i32>,
    /// self, opponent or centered, relative to the deciding player.
    pub cube_owner: Option<CubeOwnerFilter>,
    /// Order by equity loss; the largest loss comes first by default.
    pub direction: Option<LossDirection>,
    /// One-based page.
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LossDirection {
    Asc,
    #[default]
    Desc,
}

impl LossDirection {
    /// Both orders follow decision_index_player_loss: descending reads it
    /// forward, ascending reverses every key so it is read backward.
    fn order_by(self) -> &'static str {
        match self {
            Self::Desc => "d.equity_loss DESC,d.match_id,d.game_no,d.decision_no",
            Self::Asc => "d.equity_loss,d.match_id DESC,d.game_no DESC,d.decision_no DESC",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CubeOwnerFilter {
    #[serde(rename = "self")]
    Own,
    Opponent,
    Centered,
}

#[derive(Serialize, ToSchema)]
pub struct DecisionHit {
    #[serde(flatten)]
    pub question: MistakePracticeQuestion,
    pub game_plan: GamePlan,
    /// Error type when the played action was not best.
    pub mistake_kind: Option<MistakeKind>,
    pub played: String,
    pub best: String,
    pub equity_loss: f64,
    /// 0 is money play.
    pub match_length: i32,
    /// Points still needed by the deciding player and the opponent.
    pub away: Option<[i32; 2]>,
    pub crawford: bool,
    pub cube_value: i32,
    /// Absolute color; absent while the cube is centered.
    pub cube_owner: Option<i32>,
    /// True when the practice endpoints accept this reference.
    pub practicable: bool,
}

#[derive(Serialize, ToSchema)]
pub struct DecisionSearchPage {
    #[schema(value_type = Vec<DecisionHit>)]
    pub items: Vec<serde_json::Value>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
}

pub(super) struct Context {
    pub(super) match_length: i32,
    pub(super) away: Option<[i32; 2]>,
    pub(super) crawford: bool,
    pub(super) cube_value: i32,
    pub(super) cube_owner: Option<i32>,
}

pub(super) fn context(row: &SourceDecision) -> Option<Context> {
    let player = row.decision.player;
    let decoded = cube_context(&row.question.position.gnubg_match_id, player).ok()?;
    let own = usize::try_from(player).ok()?;
    Some(Context {
        match_length: decoded.match_length,
        away: decoded.away.map(|away| [away[own], away[1 - own]]),
        crawford: decoded.crawford,
        cube_value: decoded.cube_value,
        cube_owner: decoded.cube_owner,
    })
}

/// Cube ownership relative to the deciding player.
pub(super) fn owner_filter(context: &Context, player: i32) -> CubeOwnerFilter {
    match context.cube_owner {
        None => CubeOwnerFilter::Centered,
        Some(owner) if owner == player => CubeOwnerFilter::Own,
        Some(_) => CubeOwnerFilter::Opponent,
    }
}

impl CubeOwnerFilter {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Own => "self",
            Self::Opponent => "opponent",
            Self::Centered => "centered",
        }
    }
}

impl DecisionHit {
    pub(super) fn new(row: SourceDecision, context: &Context, practicable: bool) -> Self {
        let error = (row.decision.equity_loss > 0.0).then(|| mistake_kind(&row.decision));
        Self {
            game_plan: row.plan,
            mistake_kind: error,
            match_length: context.match_length,
            away: context.away,
            crawford: context.crawford,
            cube_value: context.cube_value,
            cube_owner: context.cube_owner,
            practicable,
            played: row.decision.played,
            best: row.decision.best,
            equity_loss: row.decision.equity_loss,
            question: row.question,
        }
    }
}

/// Shared WHERE clause; $1 player, $2 database, $3 kind, $4 mistake kind,
/// $5 game plan, $6/$7 loss range, $8/$9 away scores, $10/$11 cube.
const FILTER: &str = "d.player_name=$1
   AND ($2::uuid IS NULL OR m.database_id=$2)
   AND ($3::text IS NULL OR d.question_kind=$3)
   AND ($4::text IS NULL OR (d.equity_loss>0 AND d.mistake_kind=$4))
   AND ($5::text IS NULL OR d.game_plan=$5)
   AND d.equity_loss>=$6
   AND ($7::float8 IS NULL OR d.equity_loss<=$7)
   AND ($8::int IS NULL OR d.away_own=$8)
   AND ($9::int IS NULL OR d.away_opponent=$9)
   AND ($10::int IS NULL OR d.cube_value=$10)
   AND ($11::text IS NULL OR d.cube_owner=$11)";

#[utoipa::path(get, path = "/api/study/decisions", params(DecisionSearchQuery),
    responses((status = 200, body = DecisionSearchPage), (status = 422, body = ErrorBody)),
    tag = "Study")]
pub(super) async fn search(
    State(s): State<AppState>,
    input: Result<Query<DecisionSearchQuery>, QueryRejection>,
) -> ApiResult<Json<DecisionSearchPage>> {
    let q = input.map_err(|e| ApiError::invalid(e.body_text()))?.0;
    let (player, min_loss) = validate(Some(q.player.clone()), q.min_loss.or(Some(0.0)))?;
    let player = player.unwrap_or_default().to_lowercase();
    if q.max_loss
        .is_some_and(|max| !max.is_finite() || max < min_loss)
    {
        return Err(ApiError::invalid(
            "max_loss must be finite and at least min_loss",
        ));
    }
    if [q.away, q.opponent_away]
        .into_iter()
        .flatten()
        .any(|away| !(1..=64).contains(&away))
        || q.cube_value
            .is_some_and(|value| !(1..=4096).contains(&value) || value.count_ones() != 1)
    {
        return Err(ApiError::invalid(
            "Away scores must be 1 to 64 and the cube value a power of two up to 4096",
        ));
    }
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT);
    let page = q.page.unwrap_or(1);
    if !(1..=MAX_LIMIT).contains(&limit) || !(1..=MAX_PAGE).contains(&page) {
        return Err(ApiError::invalid(
            "limit must be 1 to 100 and page between 1 and 100000",
        ));
    }
    if let Some(id) = q.database_id {
        super::source::require_database(&s.pool, id).await?;
    }
    super::index::ensure(&s.pool, q.database_id, &player).await?;
    let kind = q.kind.map(DecisionKind::name);
    let mistake = q.mistake_kind.map(super::history::kind_name);
    let plan = q.game_plan.map(crate::position_game_plans::label);
    let owner = q.cube_owner.map(CubeOwnerFilter::name);
    // The order matches decision_index_player_loss, so a page is read in index order.
    let order = q.direction.unwrap_or_default().order_by();
    let page_sql = format!(
        "SELECT d.hit FROM decision_index d JOIN matches m ON m.id=d.match_id
         WHERE {FILTER}
         ORDER BY {order}
         LIMIT $12 OFFSET $13"
    );
    let items: Vec<serde_json::Value> = sqlx::query_scalar(&page_sql)
        .bind(&player)
        .bind(q.database_id)
        .bind(kind)
        .bind(mistake)
        .bind(plan)
        .bind(min_loss)
        .bind(q.max_loss)
        .bind(q.away)
        .bind(q.opponent_away)
        .bind(q.cube_value)
        .bind(owner)
        .bind(i64::try_from(limit).map_err(anyhow::Error::from)?)
        .bind(i64::try_from((page - 1) * limit).map_err(anyhow::Error::from)?)
        .fetch_all(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
    let count_sql = format!(
        "SELECT count(*) FROM decision_index d JOIN matches m ON m.id=d.match_id WHERE {FILTER}"
    );
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(&player)
        .bind(q.database_id)
        .bind(kind)
        .bind(mistake)
        .bind(plan)
        .bind(min_loss)
        .bind(q.max_loss)
        .bind(q.away)
        .bind(q.opponent_away)
        .bind(q.cube_value)
        .bind(owner)
        .fetch_one(&s.pool)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(Json(DecisionSearchPage {
        items,
        total: usize::try_from(total).unwrap_or(0),
        page,
        limit,
    }))
}
