//! Checked practice answers by day and mistake group.
use super::{MistakeKind, kind, source::SourceDecision};
use crate::json_errors::Json;
use crate::{
    api::{ApiResult, AppState},
    game_plan::GamePlan,
    practice::evaluation::PracticeEvaluation,
};
use axum::extract::State;
use chrono::NaiveDate;
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use utoipa::ToSchema;

const MAX_ROWS: i64 = 500;

pub(super) fn kind_name(kind: MistakeKind) -> &'static str {
    match kind {
        MistakeKind::Checker => "checker",
        MistakeKind::MissedDouble => "missed_double",
        MistakeKind::PrematureDouble => "premature_double",
        MistakeKind::WrongTake => "wrong_take",
        MistakeKind::WrongPass => "wrong_pass",
        MistakeKind::UnclassifiedCube => "unclassified_cube",
    }
}

/// Keep one row per checked answer. Reveals are not attempts.
pub(super) async fn record(
    pool: &PgPool,
    row: &SourceDecision,
    evaluation: &PracticeEvaluation,
) -> ApiResult<()> {
    let (verdict, loss) = evaluation.outcome();
    let player = usize::try_from(row.decision.player)
        .ok()
        .and_then(|index| row.question.players.get(index))
        .map(|name| name.trim())
        .unwrap_or_default();
    sqlx::query(
        "INSERT INTO mistake_practice_attempts(player,game_plan,mistake_kind,verdict,equity_loss)
         VALUES($1,$2,$3,$4,$5)",
    )
    .bind(player)
    .bind(crate::position_game_plans::label(row.plan))
    .bind(kind_name(kind(&row.decision)))
    .bind(verdict)
    .bind(loss.max(0.0))
    .execute(pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(())
}

#[derive(Serialize, FromRow, ToSchema)]
pub struct MistakePracticeDay {
    /// Day in the database server's time zone.
    pub day: NaiveDate,
    pub player: String,
    #[schema(value_type = GamePlan)]
    pub game_plan: String,
    #[schema(value_type = MistakeKind)]
    pub mistake_kind: String,
    pub attempts: i64,
    /// Answers that matched the best decision.
    pub best: i64,
    pub average_equity_loss: f64,
}

#[derive(Serialize, ToSchema)]
pub struct MistakePracticeHistory {
    /// Newest day first; at most 500 day-and-group rows.
    pub items: Vec<MistakePracticeDay>,
}

#[utoipa::path(get, path = "/api/study/mistakes/history",
    responses((status = 200, body = MistakePracticeHistory)), tag = "Study")]
pub(super) async fn list(State(s): State<AppState>) -> ApiResult<Json<MistakePracticeHistory>> {
    let items = sqlx::query_as(
        "SELECT attempted_at::date AS day,player,game_plan,mistake_kind,
                count(*) AS attempts,
                count(*) FILTER (WHERE verdict='best') AS best,
                avg(equity_loss)::double precision AS average_equity_loss
         FROM mistake_practice_attempts
         GROUP BY 1,2,3,4
         ORDER BY 1 DESC,2,3,4 LIMIT $1",
    )
    .bind(MAX_ROWS)
    .fetch_all(&s.pool)
    .await
    .map_err(anyhow::Error::from)?;
    Ok(Json(MistakePracticeHistory { items }))
}
