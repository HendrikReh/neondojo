use super::{ApiError, ApiResult, AppState, ErrorBody};
use crate::json_errors::{Json, Multipart, Path, Query};
use crate::{
    engine::{Engine, valid_id},
    import::{self, MatchFormat},
    model::*,
    store,
};
use axum::{
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Deserialize, IntoParams)]
pub struct ImportQuery {
    /// Target logical database. Omitted: default database.
    pub database_id: Option<Uuid>,
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub(super) struct UploadForm {
    /// One MAT (also as a .txt match transcript), SGF, binary XG, BGBlitz BGF, or HedgeHog OGXM file, at most 16 MiB including multipart envelope.
    #[schema(value_type=String, format=Binary)]
    file: String,
}

#[utoipa::path(post, path="/api/imports", params(ImportQuery), request_body(content=UploadForm, content_type="multipart/form-data"), responses((status=201, body=ImportResponse), (status=200, body=ImportResponse, description="Previously imported file"), (status=409, body=ErrorBody, description="Target is reserved for paused sessions"), (status=422, body=ErrorBody), (status=413, description="Upload too large"), (status=429, body=ErrorBody)), tag="Library")]
pub(super) async fn upload(
    State(state): State<AppState>,
    Query(query): Query<ImportQuery>,
    Multipart(mut multipart): Multipart,
) -> ApiResult<(StatusCode, Json<ImportResponse>)> {
    let _permit = state.imports.try_acquire().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Two imports are already in progress".into(),
        )
    })?;
    let field = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(e.status(), e.body_text().into()))?
        .ok_or_else(|| ApiError::invalid("Missing file field"))?;
    if field.name() != Some("file") {
        return Err(ApiError::invalid("Expected multipart field 'file'"));
    }
    let filename = field
        .file_name()
        .ok_or_else(|| ApiError::invalid("Missing filename"))?
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_owned();
    if filename.len() > 255 || filename.is_empty() || filename.chars().any(char::is_control) {
        return Err(ApiError::invalid("Invalid filename"));
    }
    let extension = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let format = MatchFormat::from_extension(&extension).ok_or_else(|| {
        ApiError::invalid(format!(
            "Supported formats: {}",
            import::supported_extensions()
        ))
    })?;
    let bytes = field
        .bytes()
        .await
        .map_err(|e| ApiError(e.status(), e.body_text().into()))?;
    if bytes.is_empty() {
        return Err(ApiError::invalid("Empty file"));
    }
    if multipart
        .next_field()
        .await
        .map_err(|e| ApiError(e.status(), e.body_text().into()))?
        .is_some()
    {
        return Err(ApiError::invalid("Upload one file per request"));
    }
    let gnubg = async {
        let settings = crate::settings::load(&state).await?;
        Ok(Engine::new(settings.gnubg_path))
    };
    let import::ConvertedMatch { sgf, warnings } = format
        .convert(bytes.clone(), gnubg)
        .await?
        .map_err(|e| ApiError::invalid(format.conversion_error(&extension, &e)))?;
    let parse_source = sgf.clone();
    let imported = tokio::task::spawn_blocking(move || import::parse_sgf(&parse_source))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|e| ApiError::invalid(format!("Invalid game: {e:#}")))?;
    if imported.positions.is_empty() || imported.positions.len() > 100_000 {
        return Err(ApiError::invalid("Expected 1–100000 checker moves"));
    }
    let database_id = match query.database_id {
        Some(id) => id,
        None => crate::databases::default_database_id(&state.pool)
            .await
            .map_err(anyhow::Error::from)?,
    };
    crate::databases::require_library_destination(&state.pool, database_id).await?;
    let mut result = store::import_source_in_database(
        &state.pool,
        &filename,
        format.id(),
        &bytes,
        &sgf,
        &imported,
        warnings.clone(),
        database_id,
    )
    .await?;
    // Conversion warnings describe the source file, so a duplicate upload of
    // the same file reports them too.
    result.warnings = warnings;
    if !result.duplicate {
        state.browse_wake.notify_one();
    }
    Ok((
        if result.duplicate {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        Json(result),
    ))
}

#[derive(Deserialize, IntoParams)]
pub(super) struct MatchQuery {
    q: Option<String>,
    after: Option<Uuid>,
    limit: Option<i64>,
}

pub(super) fn limit(value: Option<i64>) -> ApiResult<i64> {
    let limit = value.unwrap_or(50);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::invalid("limit must be between 1 and 100"));
    }
    Ok(limit)
}

#[utoipa::path(get, path="/api/matches", params(MatchQuery), responses((status=200, body=MatchPage), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn matches(
    State(state): State<AppState>,
    Query(query): Query<MatchQuery>,
) -> ApiResult<Json<MatchPage>> {
    let q = query.q.as_deref().unwrap_or("").trim();
    if !q.is_empty() && !(3..=200).contains(&q.chars().count()) {
        return Err(ApiError::invalid("Search needs 3–200 characters"));
    }
    Ok(Json(
        store::matches(&state.pool, q, query.after, limit(query.limit)?).await?,
    ))
}

#[utoipa::path(get, path="/api/matches/{id}", params(("id"=Uuid, Path)), responses((status=200, body=MatchSummary), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn get_match(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MatchSummary>> {
    crate::play::require_review_available(&state.pool, id).await?;
    Ok(Json(
        store::get_match(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}

#[utoipa::path(get, path="/api/matches/{id}/games", params(("id"=Uuid, Path)), responses((status=200, body=Vec<MatchGame>), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn match_games(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<MatchGame>>> {
    crate::play::require_review_available(&state.pool, id).await?;
    Ok(Json(
        store::match_games(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}

/// Irregularities in the source file that the import resolved, in game order.
#[utoipa::path(get, path="/api/matches/{id}/import-warnings", params(("id"=Uuid, Path)), responses((status=200, body=Vec<ImportWarning>), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn match_import_warnings(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ImportWarning>>> {
    crate::play::require_review_available(&state.pool, id).await?;
    Ok(Json(
        store::import_warnings(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}

#[utoipa::path(get, path="/api/positions/{id}", params(("id"=Uuid, Path)), responses((status=200, body=Position), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn get_position(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Position>> {
    crate::play::require_position_review_available(&state.pool, id).await?;
    Ok(Json(
        store::get_position(&state.pool, id)
            .await?
            .ok_or_else(ApiError::missing)?,
    ))
}

#[derive(Deserialize, IntoParams)]
pub(super) struct GameScope {
    /// One-based source game number; omitted means the whole match.
    pub(super) game_no: Option<i32>,
}

pub(super) async fn validate_game(
    state: &AppState,
    match_id: Uuid,
    game_no: Option<i32>,
) -> ApiResult<Vec<MatchGame>> {
    // Existence and scope come from the catalog; results are not needed here.
    let games = store::game_catalog(&state.pool, match_id)
        .await?
        .ok_or_else(ApiError::missing)?;
    if game_no.is_some_and(|n| !games.iter().any(|g| g.game_no == n)) {
        return Err(ApiError::invalid("game_no is outside this match"));
    }
    Ok(games)
}

#[utoipa::path(get, path="/api/matches/{id}/source", params(("id"=Uuid, Path)), responses((status=200, body=String, content_type="application/x-go-sgf"), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn source(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Response> {
    crate::play::require_review_available(&state.pool, id).await?;
    let sgf: Option<String> =
        sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1 AND NOT collection_only")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-go-sgf"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=match.sgf",
            ),
        ],
        sgf.ok_or_else(ApiError::missing)?,
    )
        .into_response())
}

#[derive(Deserialize, IntoParams)]
pub(super) struct PositionQuery {
    database_id: Option<Uuid>,
    match_id: Option<Uuid>,
    game_no: Option<i32>,
    position_id: Option<String>,
    after: Option<Uuid>,
    limit: Option<i64>,
}

#[utoipa::path(get, path="/api/positions", params(PositionQuery), responses((status=200, body=PositionPage), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn positions(
    State(state): State<AppState>,
    Query(query): Query<PositionQuery>,
) -> ApiResult<Json<PositionPage>> {
    if let Some(id) = query.match_id {
        crate::play::require_review_available(&state.pool, id).await?;
    }
    if query.game_no.is_some() {
        let match_id = query
            .match_id
            .ok_or_else(|| ApiError::invalid("game_no requires match_id"))?;
        validate_game(&state, match_id, query.game_no).await?;
    }
    if let Some(ref id) = query.position_id
        && !valid_id(id, 14)
    {
        return Err(ApiError::invalid(
            "Expected a 14-character GNUbg Position ID",
        ));
    }
    if query.position_id.is_none() && query.match_id.is_none() {
        return Err(ApiError::invalid("Supply match_id or position_id"));
    }
    if let Some(cursor) = query.after {
        let p = store::get_position(&state.pool, cursor)
            .await?
            .ok_or_else(|| ApiError::invalid("Unknown cursor"))?;
        if let Some(database_id) = query.database_id {
            let belongs: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM matches WHERE id=$1 AND database_id=$2)",
            )
            .bind(p.match_uuid)
            .bind(database_id)
            .fetch_one(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
            if !belongs {
                return Err(ApiError::invalid(
                    "Cursor does not match the database filter",
                ));
            }
        }
        if query.match_id.is_some_and(|id| id != p.match_uuid)
            || query.game_no.is_some_and(|n| n != p.game_no)
            || query
                .position_id
                .as_ref()
                .is_some_and(|id| *id != p.position_id)
        {
            return Err(ApiError::invalid("Cursor does not match the filters"));
        }
    }
    Ok(Json(
        store::positions_scoped_in_database(
            &state.pool,
            query.match_id,
            query.position_id.as_deref(),
            query.after,
            limit(query.limit)?,
            query.game_no,
            query.database_id,
        )
        .await?,
    ))
}

#[utoipa::path(get, path="/api/matches/{id}/dice", params(("id"=Uuid, Path), GameScope), responses((status=200, body=DiceStats), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn dice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<GameScope>,
) -> ApiResult<Json<DiceStats>> {
    crate::play::require_review_available(&state.pool, id).await?;
    validate_game(&state, id, query.game_no).await?;
    Ok(Json(
        store::dice_stats_scoped(&state.pool, id, query.game_no).await?,
    ))
}
