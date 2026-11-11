//! Durable aggregates from completed similar-position practice rounds.
use crate::api::{ApiError, ApiResult, AppState, ErrorBody};
use axum::{
    Json, Router,
    extract::{
        Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::StatusCode,
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::{IntoParams, OpenApi, ToSchema};
use uuid::Uuid;

const PAGE_SIZE: usize = 25;
const COLUMNS: &str =
    "id, completed_at, total_positions, scored_moves, missed_moves, total_equity_loss, is_retry";

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
struct SaveSimilarPracticeSummary {
    /// Stable round ID for safe retries after a lost response.
    id: Uuid,
    /// Time the user finished the round, preserved across save retries.
    completed_at: DateTime<Utc>,
    #[schema(minimum = 1, maximum = 50)]
    total_positions: i32,
    /// One loss for every scored move, including correct plays with zero loss.
    #[schema(min_items = 1, max_items = 50)]
    losses: Vec<f64>,
    is_retry: bool,
}

#[derive(Serialize, ToSchema, FromRow)]
struct SimilarPracticeSummary {
    id: Uuid,
    completed_at: DateTime<Utc>,
    total_positions: i32,
    scored_moves: i32,
    missed_moves: i32,
    total_equity_loss: f64,
    #[sqlx(skip)]
    average_equity_loss: f64,
    is_retry: bool,
}

impl SimilarPracticeSummary {
    fn with_average(mut self) -> Self {
        self.average_equity_loss = self.total_equity_loss / f64::from(self.scored_moves);
        self
    }
}

#[derive(Deserialize, IntoParams)]
#[serde(deny_unknown_fields)]
#[into_params(parameter_in = Query)]
struct HistoryQuery {
    /// Pass next_cursor to continue from the previous page.
    before: Option<Uuid>,
}

#[derive(Serialize, ToSchema)]
struct SimilarPracticeHistory {
    items: Vec<SimilarPracticeSummary>,
    next_cursor: Option<Uuid>,
}

pub(super) fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/study/similar-practice",
        get(list_similar_practice_history).post(save_similar_practice_summary),
    )
}

#[utoipa::path(post, path="/api/study/similar-practice",request_body=SaveSimilarPracticeSummary,
    responses((status=200,body=SimilarPracticeSummary),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Study")]
async fn save_similar_practice_summary(
    State(state): State<AppState>,
    input: Result<Json<SaveSimilarPracticeSummary>, JsonRejection>,
) -> ApiResult<Json<SimilarPracticeSummary>> {
    let Json(round) = input.map_err(|error| ApiError::invalid(error.body_text()))?;
    let completed_at = DateTime::from_timestamp_millis(round.completed_at.timestamp_millis())
        .ok_or_else(|| ApiError::invalid("Invalid practice completion time"))?;
    if !(1..=super::RESULT_LIMIT as i32).contains(&round.total_positions)
        || round.losses.is_empty()
        || round.losses.len() > round.total_positions as usize
        || round
            .losses
            .iter()
            .any(|loss| !loss.is_finite() || *loss < 0.0)
    {
        return Err(ApiError::invalid(
            "Supply one nonnegative equity loss per scored move within the round",
        ));
    }
    let total: f64 = round.losses.iter().sum();
    if !total.is_finite() {
        return Err(ApiError::invalid("Total equity loss must be finite"));
    }
    let scored = round.losses.len() as i32;
    let missed = round.losses.iter().filter(|loss| **loss > 0.0).count() as i32;
    sqlx::query("INSERT INTO similar_practice_summaries(id,completed_at,total_positions,scored_moves,missed_moves,total_equity_loss,is_retry)
                 VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING")
        .bind(round.id).bind(completed_at).bind(round.total_positions).bind(scored).bind(missed).bind(total).bind(round.is_retry)
        .execute(&state.pool).await.map_err(anyhow::Error::from)?;
    let saved: SimilarPracticeSummary = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM similar_practice_summaries WHERE id=$1"
    ))
    .bind(round.id)
    .fetch_one(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    if saved.completed_at != completed_at
        || saved.total_positions != round.total_positions
        || saved.scored_moves != scored
        || saved.missed_moves != missed
        || saved.total_equity_loss != total
        || saved.is_retry != round.is_retry
    {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "This practice round was already saved with a different summary".into(),
        ));
    }
    Ok(Json(saved.with_average()))
}

#[utoipa::path(get, path="/api/study/similar-practice",params(HistoryQuery),
    responses((status=200,body=SimilarPracticeHistory),(status=422,body=ErrorBody)),tag="Study")]
async fn list_similar_practice_history(
    State(state): State<AppState>,
    query: Result<Query<HistoryQuery>, QueryRejection>,
) -> ApiResult<Json<SimilarPracticeHistory>> {
    let Query(query) = query.map_err(|error| ApiError::invalid(error.body_text()))?;
    let mut items: Vec<SimilarPracticeSummary> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM similar_practice_summaries
         WHERE $1::uuid IS NULL OR (completed_at,id) < (SELECT completed_at,id FROM similar_practice_summaries WHERE id=$1)
         ORDER BY completed_at DESC,id DESC LIMIT $2"
    )).bind(query.before).bind((PAGE_SIZE + 1) as i64).fetch_all(&state.pool).await.map_err(anyhow::Error::from)?;
    let next_cursor = if items.len() > PAGE_SIZE {
        items.truncate(PAGE_SIZE);
        items.last().map(|item| item.id)
    } else {
        None
    };
    Ok(Json(SimilarPracticeHistory {
        items: items
            .into_iter()
            .map(SimilarPracticeSummary::with_average)
            .collect(),
        next_cursor,
    }))
}

#[derive(OpenApi)]
#[openapi(
    paths(save_similar_practice_summary, list_similar_practice_history),
    components(schemas(
        SaveSimilarPracticeSummary,
        SimilarPracticeSummary,
        SimilarPracticeHistory
    ))
)]
pub(crate) struct SimilarPracticeHistoryDoc;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use serde_json::{Value, json};
    use tower::ServiceExt;

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn saves_rounds_once_and_pages_through_equal_completion_times(pool: sqlx::PgPool) {
        let app = crate::api::router(AppState::new(
            pool,
            crate::engine::Engine::new("unused".into()),
        ));
        let request = |method, path: String, body: Value| {
            let app = app.clone();
            async move {
                let response = app
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(path)
                            .header("content-type", "application/json")
                            .body(Body::from(body.to_string()))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                let status = response.status();
                let body: Value = serde_json::from_slice(
                    &to_bytes(response.into_body(), usize::MAX).await.unwrap(),
                )
                .unwrap();
                (status, body)
            }
        };
        let path = "/api/study/similar-practice";
        let round = |id| {
            json!({
                "id": Uuid::from_u128(id),
                "completed_at": "2026-01-01T12:00:00.123Z",
                "total_positions": 3,
                "losses": [0.25, 0.0],
                "is_retry": false,
            })
        };
        let post = |body| request(axum::http::Method::POST, path.into(), body);
        let get = |uri: String| request(axum::http::Method::GET, uri, Value::Null);

        let (status, saved) = post(round(1)).await;
        assert_eq!(status, StatusCode::OK, "{saved}");
        assert_eq!(saved["scored_moves"], 2);
        assert_eq!(saved["missed_moves"], 1);
        assert_eq!(saved["average_equity_loss"], 0.125);
        assert_eq!(post(round(1)).await, (StatusCode::OK, saved));

        let mut conflict = round(1);
        conflict["is_retry"] = json!(true);
        assert_eq!(post(conflict).await.0, StatusCode::CONFLICT);
        let mut invalid = round(27);
        invalid["losses"] = json!([]);
        assert_eq!(post(invalid).await.0, StatusCode::UNPROCESSABLE_ENTITY);

        for id in 2..=26 {
            let (status, body) = post(round(id)).await;
            assert_eq!(status, StatusCode::OK, "{body}");
        }
        let (status, first) = get(path.into()).await;
        assert_eq!(status, StatusCode::OK, "{first}");
        let ids = |page: &Value| {
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| {
                    item["id"]
                        .as_str()
                        .unwrap()
                        .parse::<Uuid>()
                        .unwrap()
                        .as_u128()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&first), (2..=26).rev().collect::<Vec<_>>());
        let cursor = first["next_cursor"].as_str().unwrap();
        let (status, second) = get(format!("{path}?before={cursor}")).await;
        assert_eq!(status, StatusCode::OK, "{second}");
        assert_eq!(ids(&second), [1]);
        assert!(second["next_cursor"].is_null());
    }
}
