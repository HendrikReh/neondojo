use super::{
    Competitor, CreateSeries, DiceMode, DuelFormat, ENGINE_DUELS_DATABASE_ID, GameOutcomes,
    MAX_BLOCK_GAMES, MatchResult, MoneyStatistics, Series, SeriesConfig, SeriesDetails,
    SeriesEngines, SeriesPage, SeriesStatistics, SideCostSummary, dice, runner, summarize,
};
use crate::domain_error::DomainError;
use crate::json_errors::{Json, Path, Query};
use crate::{
    api::{ApiError, ApiResult, AppState, ErrorBody},
    engine::{Engine, resolve_binary_async},
    engines::EngineId,
    model::MatchSummary,
    settings::{self, EngineSnapshot},
    store,
};
use axum::{
    Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::PgPool;
use utoipa::OpenApi;
use uuid::Uuid;

#[derive(Deserialize)]
struct HistoryQuery {
    before: Option<Uuid>,
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
        .route(
            "/api/engine-matches/{id}/matches/{index}/library",
            post(library_match),
        )
}

pub(crate) async fn list_series(
    pool: &PgPool,
    before: Option<Uuid>,
) -> Result<SeriesPage, DomainError> {
    let mut runs: Vec<Series> = sqlx::query_as(
        "SELECT * FROM engine_match_runs
         WHERE $1::uuid IS NULL OR (created_at,id) <
             (SELECT created_at,id FROM engine_match_runs WHERE id=$1)
         ORDER BY created_at DESC,id DESC LIMIT $2",
    )
    .bind(before)
    .bind(PAGE_SIZE as i64 + 1)
    .fetch_all(pool)
    .await
    .map_err(anyhow::Error::from)?;
    let next_cursor = if runs.len() > PAGE_SIZE {
        runs.truncate(PAGE_SIZE);
        runs.last().map(|run| run.id)
    } else {
        None
    };
    Ok(SeriesPage { runs, next_cursor })
}

pub(crate) async fn create_series(
    state: &AppState,
    request: CreateSeries,
) -> Result<Series, DomainError> {
    if !(1..=1000).contains(&request.count) {
        return Err(DomainError::invalid("Count must be 1–1000"));
    }
    let valid_format = match request.format {
        DuelFormat::Match => {
            (1..=25).contains(&request.match_length) && request.block_games.is_none()
        }
        DuelFormat::MoneyCubeless => {
            request.match_length == 0
                && request
                    .block_games
                    .is_some_and(|games| (1..=MAX_BLOCK_GAMES).contains(&games))
        }
    };
    if !valid_format {
        return Err(DomainError::invalid(
            "Matches need a length of 1–25 and no block size; cubeless money blocks need 1–1000 games and no match length",
        ));
    }
    let cubeless = request.format == DuelFormat::MoneyCubeless;
    for side in [&request.a, &request.b] {
        if !EngineId::parse(&side.engine_id)
            .is_some_and(|engine| engine.depths("duel").contains(&side.plies))
        {
            return Err(DomainError::invalid(
                "Unsupported playing engine or strength",
            ));
        }
    }
    let settings = settings::load(state).await?;
    let path = resolve_binary_async(&settings.gnubg_path)
        .await
        .map_err(|e| DomainError::invalid(e.to_string()))?;
    let gnubg_threads = settings.gnubg_threads.filter(|&threads| threads != 1);
    let version = Engine::new(path.clone())
        .with_threads(gnubg_threads)
        .check_installation()
        .await
        .map_err(|error| {
            DomainError::invalid(format!("GNUbg installation check failed: {error:#}"))
        })?;
    crate::play::probe_native(
        &Engine::new(path.clone())
            .with_threads(gnubg_threads)
            .with_expected_version(Some(version.clone())),
    )
    .await
    .map_err(|error| {
        DomainError::invalid(format!("GNUbg play installation check failed: {error:#}"))
    })?;
    let sage_threads = settings.sage_analysis_threads();
    let snapshot = |side: &Competitor| {
        let path = path.clone();
        let version = version.clone();
        let id = side.engine_id.clone();
        let plies = side.plies;
        async move {
            // The rules engine side reuses the installation checked above;
            // other engines (IDs checked against their duel depths) verify theirs.
            if EngineId::parse(&id).is_some_and(EngineId::is_rules_engine) {
                Ok::<_, DomainError>(EngineSnapshot {
                    engine_id: id,
                    gnubg_path: path,
                    engine_version: Some(version),
                    gnubg_threads,
                    gnubg_move_filter: None,
                    open_sage: None,
                })
            } else {
                let mut snapshot =
                    settings::snapshot_for(state, &id, &path, None, sage_threads, plies)
                        .await
                        .map_err(|error| {
                            DomainError::invalid(format!(
                                "NeonGammon installation check failed: {error:#}"
                            ))
                        })?;
                if cubeless {
                    snapshot = settings::cubeless_sage(snapshot).await.map_err(|error| {
                        DomainError::invalid(format!("NeonGammon cubeless check failed: {error:#}"))
                    })?;
                }
                Ok(snapshot)
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
        format: request.format,
        match_length: request.match_length,
        block_games: request.block_games,
        count: request.count,
        mode: request.mode,
        seed: request.seed.unwrap_or_else(Uuid::new_v4),
    };
    let series: Series = sqlx::query_as("INSERT INTO engine_match_runs(id,configuration,engines,dice_version,total_matches) VALUES($1,$2,$3,$4,$5) RETURNING *")
        .bind(Uuid::new_v4())
        .bind(sqlx::types::Json(&configuration))
        .bind(sqlx::types::Json(&engines))
        .bind(dice::VERSION)
        .bind(configuration.count * configuration.mode.matches_per_unit())
        .fetch_one(&state.pool).await.map_err(anyhow::Error::from)?;
    state.notify_engine_match_update(series.id);
    Ok(series)
}

pub(crate) async fn series_details(
    pool: &PgPool,
    id: Uuid,
    offset: i32,
) -> Result<SeriesDetails, DomainError> {
    if !(0..=2000).contains(&offset) {
        return Err(DomainError::invalid("Invalid result offset"));
    }
    let offset_usize = offset as usize;
    let series = load(pool, id).await?;
    // Read only committed results up to this version of the progress counter.
    let mut results: Vec<MatchResult> = sqlx::query_as("SELECT match_index,seed,score_a,score_b,games,cpu_us_a,wall_us_a,moves_a,cpu_us_b,wall_us_b,moves_b,outcomes FROM engine_match_results WHERE run_id=$1 AND match_index<$2 ORDER BY match_index")
        .bind(id).bind(series.completed_matches).fetch_all(pool).await.map_err(anyhow::Error::from)?;
    let statistics = summarize(
        series.configuration.format,
        series.configuration.mode,
        &results,
    );
    let next_offset =
        (offset_usize.saturating_add(PAGE_SIZE) < results.len()).then(|| offset + PAGE_SIZE as i32);
    let previous_offset = (offset_usize > 0).then(|| (offset - PAGE_SIZE as i32).max(0));
    let matches = results
        .drain(offset_usize.min(results.len())..)
        .take(PAGE_SIZE)
        .collect();
    Ok(SeriesDetails {
        series,
        statistics,
        matches,
        next_offset,
        previous_offset,
    })
}

pub(crate) async fn cancel_series(state: &AppState, id: Uuid) -> Result<Series, DomainError> {
    sqlx::query("UPDATE engine_match_runs SET status='cancelled',lease_token=NULL,lease_until=NULL WHERE id=$1 AND status IN ('queued','running')")
        .bind(id).execute(&state.pool).await.map_err(anyhow::Error::from)?;
    let series = load(&state.pool, id).await?;
    state.notify_engine_match_update(series.id);
    Ok(series)
}

pub(crate) async fn delete_series(state: &AppState, id: Uuid) -> Result<(), DomainError> {
    let deleted = sqlx::query(
        "DELETE FROM engine_match_runs WHERE id=$1 AND status NOT IN ('queued','running')",
    )
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(anyhow::Error::from)?
    .rows_affected();
    if deleted > 0 {
        state.notify_engine_match_update(id);
        return Ok(());
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM engine_match_runs WHERE id=$1)")
            .bind(id)
            .fetch_one(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
    Err(if exists {
        DomainError::conflict("Cancel the engine duel before deleting it")
    } else {
        DomainError::missing()
    })
}

pub(crate) async fn series_sgf(pool: &PgPool, id: Uuid, index: i32) -> Result<String, DomainError> {
    Ok(stored_sgf(pool, id, index).await?)
}

pub(crate) async fn series_library_match(
    pool: &PgPool,
    id: Uuid,
    index: i32,
) -> Result<MatchSummary, DomainError> {
    let source = stored_sgf(pool, id, index).await?;
    if let Some(entry) =
        store::library_match_by_source(pool, source.as_bytes(), ENGINE_DUELS_DATABASE_ID).await?
    {
        return Ok(entry);
    }
    let parse_source = source.clone();
    let imported = tokio::task::spawn_blocking(move || crate::import::parse_sgf(&parse_source))
        .await
        .map_err(anyhow::Error::from)??;
    let restored = store::import_match_in_database(
        pool,
        &runner::archive_filename(id, index),
        "sgf",
        source.as_bytes(),
        &source,
        &imported,
        ENGINE_DUELS_DATABASE_ID,
    )
    .await?;
    Ok(restored.r#match)
}

#[utoipa::path(get, path="/api/engine-matches", params(("before"=Option<Uuid>, Query)), responses((status=200, body=SeriesPage),(status=422, body=ErrorBody)), tag="Engine matches")]
async fn list(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<SeriesPage>> {
    Ok(Json(list_series(&state.pool, query.before).await?))
}

#[utoipa::path(post, path="/api/engine-matches", request_body=CreateSeries, responses((status=201, body=Series),(status=422, body=ErrorBody)), tag="Engine matches")]
async fn create(
    State(state): State<AppState>,
    Json(request): Json<CreateSeries>,
) -> ApiResult<(StatusCode, Json<Series>)> {
    Ok((
        StatusCode::CREATED,
        Json(create_series(&state, request).await?),
    ))
}

#[utoipa::path(get, path="/api/engine-matches/{id}", params(("id"=Uuid, Path), ("offset"=Option<i32>, Query)), responses((status=200, body=SeriesDetails),(status=404, body=ErrorBody),(status=422, body=ErrorBody)), tag="Engine matches")]
async fn details(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(page): Query<ResultPage>,
) -> ApiResult<Json<SeriesDetails>> {
    Ok(Json(series_details(&state.pool, id, page.offset).await?))
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
    Ok(Json(cancel_series(&state, id).await?))
}

async fn stored_sgf(pool: &PgPool, id: Uuid, index: i32) -> ApiResult<String> {
    sqlx::query_scalar("SELECT sgf FROM engine_match_results WHERE run_id=$1 AND match_index=$2")
        .bind(id)
        .bind(index)
        .fetch_optional(pool)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)
}

#[utoipa::path(get, path="/api/engine-matches/{id}/matches/{index}/sgf", params(("id"=Uuid, Path),("index"=i32, Path)), responses((status=200, body=String),(status=404, body=ErrorBody)), tag="Engine matches")]
async fn sgf(
    State(state): State<AppState>,
    Path((id, index)): Path<(Uuid, i32)>,
) -> ApiResult<impl IntoResponse> {
    let source = series_sgf(&state.pool, id, index).await?;
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

/// Library entry of one played match or money block, for opening it in Analysis.
///
/// Finds the archived entry by its exact source, also after it was moved to another
/// database. An entry deleted from the library is restored under Engine Duels.
#[utoipa::path(post, path="/api/engine-matches/{id}/matches/{index}/library", params(("id"=Uuid, Path),("index"=i32, Path)), responses((status=200, body=MatchSummary),(status=404, body=ErrorBody)), tag="Engine matches")]
async fn library_match(
    State(state): State<AppState>,
    Path((id, index)): Path<(Uuid, i32)>,
) -> ApiResult<Json<MatchSummary>> {
    Ok(Json(series_library_match(&state.pool, id, index).await?))
}

#[derive(OpenApi)]
#[openapi(
    paths(list, create, details, cancel, sgf, library_match),
    components(schemas(
        CreateSeries,
        Competitor,
        DiceMode,
        DuelFormat,
        SeriesConfig,
        SeriesEngines,
        Series,
        MatchResult,
        MoneyStatistics,
        GameOutcomes,
        SideCostSummary,
        SeriesStatistics,
        SeriesDetails,
        SeriesPage
    ))
)]
pub(crate) struct EngineMatchDoc;
