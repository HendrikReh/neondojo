//! Persisted head-to-head series, with a versioned dice schedule per match or pair.
mod dice;
mod runner;
#[cfg(test)]
mod tests;

pub(crate) use runner::ENGINE_DUELS_DATABASE_ID;
pub use runner::worker;

use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::{Engine, resolve_binary},
    settings::{self, EngineSnapshot},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DiceMode {
    #[default]
    Paired,
    Random,
}

impl DiceMode {
    fn matches_per_unit(self) -> i32 {
        match self {
            Self::Paired => 2,
            Self::Random => 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
struct Competitor {
    engine_id: String,
    plies: i32,
}

#[derive(Debug, Deserialize, ToSchema)]
struct CreateSeries {
    a: Competitor,
    b: Competitor,
    #[schema(minimum = 1, maximum = 25)]
    match_length: i32,
    #[schema(minimum = 1, maximum = 1000)]
    count: i32,
    #[serde(default)]
    mode: DiceMode,
    seed: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
struct SeriesConfig {
    a: Competitor,
    b: Competitor,
    match_length: i32,
    count: i32,
    mode: DiceMode,
    seed: Uuid,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
struct SeriesEngines {
    rules_version: String,
    a: EngineSnapshot,
    b: EngineSnapshot,
}

#[derive(Debug, FromRow, Serialize, ToSchema)]
struct Series {
    id: Uuid,
    #[sqlx(json)]
    configuration: SeriesConfig,
    #[sqlx(json)]
    engines: SeriesEngines,
    dice_version: String,
    status: String,
    total_matches: i32,
    completed_matches: i32,
    error: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Serialize, ToSchema)]
struct MatchResult {
    match_index: i32,
    seed: Uuid,
    score_a: i32,
    score_b: i32,
    games: i32,
}

#[derive(Debug, Serialize, ToSchema)]
struct SeriesStatistics {
    wins_a: i32,
    wins_b: i32,
    win_rate_a: Option<f64>,
    complete_pairs: i32,
    confidence_interval_a: Option<[f64; 2]>,
}

#[derive(Serialize, ToSchema)]
struct SeriesDetails {
    series: Series,
    statistics: SeriesStatistics,
    matches: Vec<MatchResult>,
    next_offset: Option<i32>,
    previous_offset: Option<i32>,
}

#[derive(Deserialize)]
struct HistoryQuery {
    before: Option<Uuid>,
}

#[derive(Serialize, ToSchema)]
struct SeriesPage {
    runs: Vec<Series>,
    next_cursor: Option<Uuid>,
}

#[derive(Deserialize)]
struct ResultPage {
    #[serde(default)]
    offset: i32,
}

const PAGE_SIZE: usize = 50;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/engine-matches", get(list).post(create))
        .route("/api/engine-matches/{id}", get(details))
        .route("/api/engine-matches/{id}/cancel", post(cancel))
        .route("/api/engine-matches/{id}/matches/{index}/sgf", get(sgf))
}

#[utoipa::path(get, path="/api/engine-matches", params(("before"=Option<Uuid>, Query)), responses((status=200, body=SeriesPage)), tag="Engine matches")]
async fn list(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<SeriesPage>> {
    let mut runs: Vec<Series> = sqlx::query_as(
        "SELECT * FROM engine_match_runs
         WHERE $1::uuid IS NULL OR (created_at,id) <
             (SELECT created_at,id FROM engine_match_runs WHERE id=$1)
         ORDER BY created_at DESC,id DESC LIMIT $2",
    )
    .bind(query.before)
    .bind(PAGE_SIZE as i64 + 1)
    .fetch_all(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let next_cursor = if runs.len() > PAGE_SIZE {
        runs.truncate(PAGE_SIZE);
        runs.last().map(|run| run.id)
    } else {
        None
    };
    Ok(Json(SeriesPage { runs, next_cursor }))
}

#[utoipa::path(post, path="/api/engine-matches", request_body=CreateSeries, responses((status=201, body=Series),(status=422, body=ErrorBody)), tag="Engine matches")]
async fn create(
    State(state): State<AppState>,
    payload: Result<Json<CreateSeries>, JsonRejection>,
) -> ApiResult<(StatusCode, Json<Series>)> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    if !(1..=25).contains(&request.match_length) || !(1..=1000).contains(&request.count) {
        return Err(ApiError::invalid(
            "Match length must be 1–25 and count 1–1000",
        ));
    }
    for side in [&request.a, &request.b] {
        if !crate::configuration::engine_depths(&side.engine_id, "duel").contains(&side.plies) {
            return Err(ApiError::invalid("Unsupported playing engine or strength"));
        }
    }
    let settings = settings::load(&state).await?;
    let path =
        resolve_binary(&settings.gnubg_path).map_err(|e| ApiError::invalid(e.to_string()))?;
    let gnubg_threads = settings.gnubg_threads.filter(|&threads| threads != 1);
    let version = Engine::new(path.clone())
        .with_threads(gnubg_threads)
        .check_installation()
        .await
        .map_err(|error| {
            ApiError::invalid(format!("GNUbg installation check failed: {error:#}"))
        })?;
    crate::play::probe_native(
        &Engine::new(path.clone())
            .with_threads(gnubg_threads)
            .with_expected_version(Some(version.clone())),
    )
    .await
    .map_err(|error| {
        ApiError::invalid(format!("GNUbg play installation check failed: {error:#}"))
    })?;
    let sage_threads =
        settings::sage_threads(settings.open_sage_threads, settings::SAGE_ANALYSIS_THREADS);
    let snapshot = |side: &Competitor| {
        let path = path.clone();
        let version = version.clone();
        let id = side.engine_id.clone();
        let plies = side.plies;
        let state = &state;
        async move {
            if id == "gnubg" {
                Ok(EngineSnapshot {
                    engine_id: id,
                    gnubg_path: path,
                    engine_version: Some(version),
                    gnubg_threads,
                    gnubg_move_filter: None,
                    open_sage: None,
                })
            } else {
                settings::snapshot_for(state, &id, &path, None, sage_threads, plies)
                    .await
                    .map_err(|error| {
                        ApiError::invalid(format!("Open Sage installation check failed: {error:#}"))
                    })
            }
        }
    };
    let a = snapshot(&request.a).await?;
    let b = if request.a.engine_id == request.b.engine_id && request.a.plies == request.b.plies {
        a.clone()
    } else {
        snapshot(&request.b).await?
    };
    let engines = SeriesEngines {
        rules_version: version,
        a,
        b,
    };
    let configuration = SeriesConfig {
        a: request.a,
        b: request.b,
        match_length: request.match_length,
        count: request.count,
        mode: request.mode,
        seed: request.seed.unwrap_or_else(Uuid::new_v4),
    };
    let series = sqlx::query_as("INSERT INTO engine_match_runs(id,configuration,engines,dice_version,total_matches) VALUES($1,$2,$3,$4,$5) RETURNING *")
        .bind(Uuid::new_v4())
        .bind(sqlx::types::Json(&configuration))
        .bind(sqlx::types::Json(&engines))
        .bind(dice::VERSION)
        .bind(configuration.count * configuration.mode.matches_per_unit())
        .fetch_one(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok((StatusCode::CREATED, Json(series)))
}

#[utoipa::path(get, path="/api/engine-matches/{id}", params(("id"=Uuid, Path), ("offset"=Option<i32>, Query)), responses((status=200, body=SeriesDetails),(status=404, body=ErrorBody),(status=422, body=ErrorBody)), tag="Engine matches")]
async fn details(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(page): Query<ResultPage>,
) -> ApiResult<Json<SeriesDetails>> {
    if !(0..=2000).contains(&page.offset) {
        return Err(ApiError::invalid("Invalid result offset"));
    }
    let offset = page.offset as usize;
    let series = load(&state.pool, id).await?;
    // Read only committed results up to this version of the progress counter.
    let mut results: Vec<MatchResult> = sqlx::query_as("SELECT match_index,seed,score_a,score_b,games FROM engine_match_results WHERE run_id=$1 AND match_index<$2 ORDER BY match_index")
        .bind(id).bind(series.completed_matches).fetch_all(&state.pool).await.map_err(anyhow::Error::from)?;
    let statistics = summarize(series.configuration.mode, &results);
    let next_offset =
        (offset.saturating_add(PAGE_SIZE) < results.len()).then(|| page.offset + PAGE_SIZE as i32);
    let previous_offset = (offset > 0).then(|| (page.offset - PAGE_SIZE as i32).max(0));
    let matches = results
        .drain(offset.min(results.len())..)
        .take(PAGE_SIZE)
        .collect();
    Ok(Json(SeriesDetails {
        series,
        statistics,
        matches,
        next_offset,
        previous_offset,
    }))
}

async fn load(pool: &PgPool, id: Uuid) -> ApiResult<Series> {
    sqlx::query_as("SELECT * FROM engine_match_runs WHERE id=$1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)
}

#[utoipa::path(post, path="/api/engine-matches/{id}/cancel", params(("id"=Uuid, Path)), responses((status=200, body=Series),(status=404, body=ErrorBody)), tag="Engine matches")]
async fn cancel(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Json<Series>> {
    sqlx::query("UPDATE engine_match_runs SET status='cancelled',lease_token=NULL,lease_until=NULL WHERE id=$1 AND status IN ('queued','running')")
        .bind(id).execute(&state.pool).await.map_err(anyhow::Error::from)?;
    Ok(Json(load(&state.pool, id).await?))
}

#[utoipa::path(get, path="/api/engine-matches/{id}/matches/{index}/sgf", params(("id"=Uuid, Path),("index"=i32, Path)), responses((status=200, body=String),(status=404, body=ErrorBody)), tag="Engine matches")]
async fn sgf(
    State(state): State<AppState>,
    Path((id, index)): Path<(Uuid, i32)>,
) -> ApiResult<impl IntoResponse> {
    let source: String = sqlx::query_scalar(
        "SELECT sgf FROM engine_match_results WHERE run_id=$1 AND match_index=$2",
    )
    .bind(id)
    .bind(index)
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?
    .ok_or_else(ApiError::missing)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-go-sgf".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"engine-match-{id}-{index}.sgf\""),
            ),
        ],
        source,
    ))
}

fn summarize(mode: DiceMode, results: &[MatchResult]) -> SeriesStatistics {
    let complete_pairs = if mode == DiceMode::Paired {
        results.len() / 2
    } else {
        0
    };
    let counted = if mode == DiceMode::Paired {
        &results[..complete_pairs * 2]
    } else {
        results
    };
    let wins_a = counted
        .iter()
        .filter(|result| result.score_a > result.score_b)
        .count() as i32;
    let wins_b = counted.len() as i32 - wins_a;
    let win_rate_a = (!counted.is_empty()).then(|| f64::from(wins_a) / counted.len() as f64);
    // Independent observations are pair means in [0,1]. Hoeffding gives a
    // conservative finite-sample interval even when every pair splits 1–1.
    let confidence_interval_a = win_rate_a.filter(|_| complete_pairs >= 20).map(|rate| {
        let radius = (40_f64.ln() / (2.0 * complete_pairs as f64)).sqrt();
        [(rate - radius).max(0.0), (rate + radius).min(1.0)]
    });
    SeriesStatistics {
        wins_a,
        wins_b,
        win_rate_a,
        complete_pairs: complete_pairs as i32,
        confidence_interval_a,
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(list, create, details, cancel, sgf),
    components(schemas(
        CreateSeries,
        Competitor,
        DiceMode,
        SeriesConfig,
        SeriesEngines,
        Series,
        MatchResult,
        SeriesStatistics,
        SeriesDetails,
        SeriesPage
    ))
)]
pub(crate) struct EngineMatchDoc;
