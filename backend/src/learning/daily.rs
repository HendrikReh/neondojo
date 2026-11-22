//! Daily allowance for flashcards that have never been reviewed.
//!
//! Reviews that are due are always offered. A new card counts against the
//! allowance on the local day of its first review.
use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use axum::{Json, Router, extract::State, routing::get};
use chrono::{DateTime, Local, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgExecutor;
use utoipa::{OpenApi, ToSchema};

pub(super) const MAX_NEW_CARDS_PER_DAY: i32 = 1000;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LearningPreferences {
    /// New flashcards introduced per local day; 0 pauses new cards.
    #[schema(minimum = 0, maximum = 1000)]
    pub new_cards_per_day: i32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct NewCardAllowance {
    pub(super) per_day: i32,
    pub(super) introduced_today: i64,
}

impl NewCardAllowance {
    pub(super) fn remaining(self) -> i64 {
        (i64::from(self.per_day) - self.introduced_today).max(0)
    }
}

/// Start of the current day on the computer that runs NeonDojo.
fn local_day_start() -> DateTime<Utc> {
    let now = Local::now();
    now.with_time(NaiveTime::MIN)
        .earliest()
        .unwrap_or(now)
        .with_timezone(&Utc)
}

pub(super) async fn allowance(
    executor: impl PgExecutor<'_>,
) -> Result<NewCardAllowance, sqlx::Error> {
    let (per_day, introduced_today): (i32, i64) = sqlx::query_as(
        "SELECT (SELECT new_cards_per_day FROM learning_preferences),
                (SELECT count(DISTINCT r.card_id) FROM card_reviews r
                 WHERE r.reviewed_at >= $1
                   AND NOT EXISTS (SELECT 1 FROM card_reviews earlier
                                   WHERE earlier.card_id = r.card_id
                                     AND earlier.reviewed_at < $1))",
    )
    .bind(local_day_start())
    .fetch_one(executor)
    .await?;
    Ok(NewCardAllowance {
        per_day,
        introduced_today,
    })
}

pub(super) fn routes() -> Router<AppState> {
    Router::new().route("/api/study/learning-preferences", get(read).put(write))
}

#[utoipa::path(get, path = "/api/study/learning-preferences",
    responses((status = 200, body = LearningPreferences)), tag = "Learning")]
async fn read(State(state): State<AppState>) -> ApiResult<Json<LearningPreferences>> {
    let new_cards_per_day =
        sqlx::query_scalar("SELECT new_cards_per_day FROM learning_preferences")
            .fetch_one(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
    Ok(Json(LearningPreferences { new_cards_per_day }))
}

#[utoipa::path(put, path = "/api/study/learning-preferences", request_body = LearningPreferences,
    responses((status = 200, body = LearningPreferences), (status = 422, body = ErrorBody)),
    tag = "Learning")]
async fn write(
    State(state): State<AppState>,
    crate::json_errors::Json(body): crate::json_errors::Json<LearningPreferences>,
) -> ApiResult<Json<LearningPreferences>> {
    if !(0..=MAX_NEW_CARDS_PER_DAY).contains(&body.new_cards_per_day) {
        return Err(ApiError::invalid(
            "new_cards_per_day must be between 0 and 1000",
        ));
    }
    sqlx::query("UPDATE learning_preferences SET new_cards_per_day=$1, updated_at=now()")
        .bind(body.new_cards_per_day)
        .execute(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(Json(body))
}

#[derive(OpenApi)]
#[openapi(paths(read, write), components(schemas(LearningPreferences)))]
pub struct DailyDoc;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowance_never_goes_negative() {
        let allowance = |per_day, introduced_today| NewCardAllowance {
            per_day,
            introduced_today,
        };
        assert_eq!(allowance(10, 3).remaining(), 7);
        assert_eq!(allowance(10, 12).remaining(), 0);
        assert_eq!(allowance(0, 0).remaining(), 0);
        assert!(local_day_start() <= Utc::now());
    }
}
