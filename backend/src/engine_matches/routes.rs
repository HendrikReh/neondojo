use super::{
    Competitor, CreateSeries, DiceMode, DuelFormat, ENGINE_DUELS_DATABASE_ID, GameOutcomes,
    MAX_BLOCK_GAMES, MatchResult, MoneyStatistics, Series, SeriesConfig, SeriesDetails,
    SeriesEngines, SeriesPage, SeriesStatistics, SideCostSummary, checkpoint, dice, progress,
    runner, summarize,
};
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
const SERIES_QUERY: &str = "SELECT r.*, CASE WHEN r.status IN ('cancelled','failed') THEN (
    SELECT game_number FROM engine_match_game_checkpoints
    WHERE run_id=r.id AND unit_index=r.completed_matches ORDER BY game_number DESC LIMIT 1
    ) END AS saved_games_in_unit FROM engine_match_runs r";

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/engine-matches", get(list).post(create))
        .route("/api/engine-matches/{id}", get(details).delete(remove))
        .route("/api/engine-matches/{id}/cancel", post(cancel))
        .route("/api/engine-matches/{id}/resume", post(resume))
        .route("/api/engine-matches/{id}/matches/{index}/sgf", get(sgf))
        .route(
            "/api/engine-matches/{id}/matches/{index}/library",
            post(library_match),
        )
}

#[utoipa::path(get, path="/api/engine-matches", params(("before"=Option<Uuid>, Query)), responses((status=200, body=SeriesPage),(status=422, body=ErrorBody)), tag="Engine matches")]
async fn list(
    State(state): State<AppState>,
    Query(query): Query<HistoryQuery>,
) -> ApiResult<Json<SeriesPage>> {
    let mut runs: Vec<Series> = sqlx::query_as(&format!(
        "{SERIES_QUERY}
         WHERE $1::uuid IS NULL OR (r.created_at,r.id) <
             (SELECT created_at,id FROM engine_match_runs WHERE id=$1)
         ORDER BY r.created_at DESC,r.id DESC LIMIT $2",
    ))
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
    Json(request): Json<CreateSeries>,
) -> ApiResult<(StatusCode, Json<Series>)> {
    if !(1..=1000).contains(&request.count) {
        return Err(ApiError::invalid("Count must be 1–1000"));
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
        return Err(ApiError::invalid(
            "Matches need a length of 1–25 and no block size; cubeless money blocks need 1–1000 games and no match length",
        ));
    }
    let cubeless = request.format == DuelFormat::MoneyCubeless;
    for side in [&request.a, &request.b] {
        if !EngineId::parse(&side.engine_id)
            .is_some_and(|engine| engine.depths("duel").contains(&side.plies))
        {
            return Err(ApiError::invalid("Unsupported playing engine or strength"));
        }
    }
    let settings = settings::load(&state).await?;
    let path = resolve_binary_async(&settings.gnubg_path)
        .await
        .map_err(|e| ApiError::invalid(e.to_string()))?;
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
    let sage_threads = settings.sage_analysis_threads();
    let snapshot = |side: &Competitor| {
        let path = path.clone();
        let version = version.clone();
        let id = side.engine_id.clone();
        let plies = side.plies;
        let state = &state;
        async move {
            // The rules engine side reuses the installation checked above;
            // other engines (IDs checked against their duel depths) verify theirs.
            if EngineId::parse(&id).is_some_and(EngineId::is_rules_engine) {
                Ok::<_, ApiError>(EngineSnapshot {
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
                            ApiError::invalid(format!(
                                "NeonGammon installation check failed: {error:#}"
                            ))
                        })?;
                if cubeless {
                    snapshot = settings::cubeless_sage(snapshot).await.map_err(|error| {
                        ApiError::invalid(format!("NeonGammon cubeless check failed: {error:#}"))
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
    let live_progress = progress::load(&state.pool, &series)
        .await
        .map_err(anyhow::Error::from)?;
    // Read only committed results up to this version of the progress counter.
    let mut results: Vec<MatchResult> = sqlx::query_as("SELECT match_index,seed,score_a,score_b,games,cpu_us_a,wall_us_a,moves_a,cpu_us_b,wall_us_b,moves_b,outcomes FROM engine_match_results WHERE run_id=$1 AND match_index<$2 ORDER BY match_index")
        .bind(id).bind(series.completed_matches).fetch_all(&state.pool).await.map_err(anyhow::Error::from)?;
    let statistics = summarize(
        series.configuration.format,
        series.configuration.mode,
        &results,
    );
    let next_offset =
        (offset.saturating_add(PAGE_SIZE) < results.len()).then(|| page.offset + PAGE_SIZE as i32);
    let previous_offset = (offset > 0).then(|| (page.offset - PAGE_SIZE as i32).max(0));
    let matches = results
        .drain(offset.min(results.len())..)
        .take(PAGE_SIZE)
        .collect();
    Ok(Json(SeriesDetails {
        series,
        live_progress,
        statistics,
        matches,
        next_offset,
        previous_offset,
    }))
}

async fn load(pool: &PgPool, id: Uuid) -> ApiResult<Series> {
    sqlx::query_as(&format!("{SERIES_QUERY} WHERE r.id=$1"))
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

async fn resume_series(state: &AppState, id: Uuid) -> Result<Series, ApiError> {
    let series: Option<Series> = sqlx::query_as(
        "UPDATE engine_match_runs SET
             status='queued',error=NULL,attempts=0,lease_token=NULL,lease_until=NULL
         WHERE id=$1 AND status='cancelled' AND completed_matches<total_matches RETURNING *",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let Some(series) = series else {
        load(&state.pool, id).await?;
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Only cancelled engine duels with unfinished matches or blocks can be resumed".into(),
        ));
    };
    Ok(series)
}

/// Queue a cancelled series from its last committed match or money block.
#[utoipa::path(post, path="/api/engine-matches/{id}/resume", params(("id"=Uuid, Path)), responses((status=200, body=Series),(status=404, body=ErrorBody),(status=409, body=ErrorBody)), tag="Engine matches")]
async fn resume(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<Json<Series>> {
    Ok(Json(resume_series(&state, id).await?))
}

async fn delete_series(state: &AppState, id: Uuid) -> Result<(), ApiError> {
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    // Lock the series before reading its latest checkpoint so a concurrent
    // resume cannot add more games or archive the same unit during removal.
    let series: Series = sqlx::query_as("SELECT * FROM engine_match_runs WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?
        .ok_or_else(ApiError::missing)?;
    if matches!(series.status.as_str(), "queued" | "running") {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Cancel the engine duel before deleting it".into(),
        ));
    }
    if let Some(saved) = checkpoint::load(&mut *tx, id, series.completed_matches).await? {
        let archive =
            runner::prepare_archive(&series, series.completed_matches, &saved.into_sgf().await?)
                .await?;
        store::insert_match_import(&mut tx, ENGINE_DUELS_DATABASE_ID, archive.prepare().await?)
            .await?;
    }
    sqlx::query("DELETE FROM engine_match_runs WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    state.browse_wake.notify_one();
    Ok(())
}

/// Remove a stopped series. Played games stay in the match database.
#[utoipa::path(delete, path="/api/engine-matches/{id}", params(("id"=Uuid, Path)), responses((status=204),(status=404, body=ErrorBody),(status=409, body=ErrorBody)), tag="Engine matches")]
async fn remove(State(state): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    delete_series(&state, id).await?;
    Ok(StatusCode::NO_CONTENT)
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
    let source = stored_sgf(&state.pool, id, index).await?;
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
    let source = stored_sgf(&state.pool, id, index).await?;
    if let Some(entry) =
        store::library_match_by_source(&state.pool, source.as_bytes(), ENGINE_DUELS_DATABASE_ID)
            .await?
    {
        return Ok(Json(entry));
    }
    let parse_source = source.clone();
    let imported = tokio::task::spawn_blocking(move || crate::import::parse_sgf(&parse_source))
        .await
        .map_err(anyhow::Error::from)??;
    let restored = store::import_match_in_database(
        &state.pool,
        &runner::archive_filename(id, index),
        "sgf",
        source.as_bytes(),
        &source,
        &imported,
        ENGINE_DUELS_DATABASE_ID,
    )
    .await?;
    Ok(Json(restored.r#match))
}

#[derive(OpenApi)]
#[openapi(
    paths(list, create, details, cancel, resume, remove, sgf, library_match),
    components(schemas(
        CreateSeries,
        Competitor,
        DiceMode,
        DuelFormat,
        SeriesConfig,
        SeriesEngines,
        Series,
        progress::LiveProgress,
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
