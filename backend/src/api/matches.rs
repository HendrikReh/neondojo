use super::{ApiError, ApiResult, AppState, ErrorBody};
use crate::domain_error::DomainError;
use crate::json_errors::{Json, Multipart, Path, Query};
use crate::{
    engine::{Engine, valid_id},
    import::{self, MatchFormat},
    model::*,
    store,
};
use axum::{
    body::Bytes,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use std::path::Path as FsPath;
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

fn clean_import_filename(raw: &str) -> Result<String, DomainError> {
    let filename = raw.rsplit(['/', '\\']).next().unwrap_or("").to_owned();
    if filename.len() > 255 || filename.is_empty() || filename.chars().any(char::is_control) {
        return Err(DomainError::invalid("Invalid filename"));
    }
    Ok(filename)
}

async fn import_match_bytes(
    state: &AppState,
    filename: String,
    bytes: Bytes,
    database_id: Option<Uuid>,
) -> Result<ImportResponse, DomainError> {
    let extension = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let format = MatchFormat::from_extension(&extension).ok_or_else(|| {
        DomainError::invalid(format!(
            "Supported formats: {}",
            import::supported_extensions()
        ))
    })?;
    if bytes.is_empty() {
        return Err(DomainError::invalid("Empty file"));
    }
    let gnubg = async {
        let settings = crate::settings::load(state).await?;
        Ok(Engine::new(settings.gnubg_path))
    };
    let import::ConvertedMatch { sgf, warnings } = format
        .convert(bytes.clone(), gnubg)
        .await?
        .map_err(|e| DomainError::invalid(format.conversion_error(&extension, &e)))?;
    let parse_source = sgf.clone();
    let imported = tokio::task::spawn_blocking(move || import::parse_sgf(&parse_source))
        .await
        .map_err(anyhow::Error::from)?
        .map_err(|e| DomainError::invalid(format!("Invalid game: {e:#}")))?;
    if imported.positions.is_empty() || imported.positions.len() > 100_000 {
        return Err(DomainError::invalid("Expected 1–100000 checker moves"));
    }
    let database_id = match database_id {
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
    Ok(result)
}

pub(crate) async fn import_match_from_path(
    state: &AppState,
    path: &FsPath,
    database_id: Option<Uuid>,
) -> Result<ImportResponse, DomainError> {
    let _permit = state
        .imports
        .try_acquire()
        .map_err(|_| DomainError::too_many_requests("Two imports are already in progress"))?;
    let raw_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| DomainError::invalid("Missing filename"))?;
    let filename = clean_import_filename(raw_name)?;
    let bytes = tokio::fs::read(path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            DomainError::missing()
        } else {
            DomainError::invalid(format!("Cannot read match file: {error}"))
        }
    })?;
    import_match_bytes(state, filename, Bytes::from(bytes), database_id).await
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
    let raw_name = field
        .file_name()
        .ok_or_else(|| ApiError::invalid("Missing filename"))?;
    let filename = clean_import_filename(raw_name)?;
    let extension = filename
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if MatchFormat::from_extension(&extension).is_none() {
        return Err(ApiError::invalid(format!(
            "Supported formats: {}",
            import::supported_extensions()
        )));
    }
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
    let result = import_match_bytes(&state, filename, bytes, query.database_id).await?;
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

pub(crate) async fn get_match_by_id(
    state: &AppState,
    id: Uuid,
) -> Result<MatchSummary, DomainError> {
    crate::play::require_review_available(&state.pool, id).await?;
    store::get_match(&state.pool, id)
        .await?
        .ok_or_else(DomainError::missing)
}

#[utoipa::path(get, path="/api/matches/{id}", params(("id"=Uuid, Path)), responses((status=200, body=MatchSummary), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn get_match(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MatchSummary>> {
    Ok(Json(get_match_by_id(&state, id).await?))
}

pub(crate) async fn get_match_games(
    state: &AppState,
    id: Uuid,
) -> Result<Vec<MatchGame>, DomainError> {
    crate::play::require_review_available(&state.pool, id).await?;
    store::match_games(&state.pool, id)
        .await?
        .ok_or_else(DomainError::missing)
}

#[utoipa::path(get, path="/api/matches/{id}/games", params(("id"=Uuid, Path)), responses((status=200, body=Vec<MatchGame>), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn match_games(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<MatchGame>>> {
    Ok(Json(get_match_games(&state, id).await?))
}

pub(crate) async fn get_match_import_warnings(
    state: &AppState,
    id: Uuid,
) -> Result<Vec<ImportWarning>, DomainError> {
    crate::play::require_review_available(&state.pool, id).await?;
    store::import_warnings(&state.pool, id)
        .await?
        .ok_or_else(DomainError::missing)
}

/// Irregularities in the source file that the import resolved, in game order.
#[utoipa::path(get, path="/api/matches/{id}/import-warnings", params(("id"=Uuid, Path)), responses((status=200, body=Vec<ImportWarning>), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn match_import_warnings(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<ImportWarning>>> {
    Ok(Json(get_match_import_warnings(&state, id).await?))
}

pub(crate) async fn get_position_by_id(
    state: &AppState,
    id: Uuid,
) -> Result<Position, DomainError> {
    crate::play::require_position_review_available(&state.pool, id).await?;
    store::get_position(&state.pool, id)
        .await?
        .ok_or_else(DomainError::missing)
}

#[utoipa::path(get, path="/api/positions/{id}", params(("id"=Uuid, Path)), responses((status=200, body=Position), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn get_position(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Position>> {
    Ok(Json(get_position_by_id(&state, id).await?))
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct GameScope {
    /// One-based source game number; omitted means the whole match.
    pub game_no: Option<i32>,
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

pub(crate) async fn get_match_source_sgf(
    state: &AppState,
    id: Uuid,
) -> Result<String, DomainError> {
    crate::play::require_review_available(&state.pool, id).await?;
    let sgf: Option<String> =
        sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1 AND NOT collection_only")
            .bind(id)
            .fetch_optional(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
    sgf.ok_or_else(DomainError::missing)
}

#[utoipa::path(get, path="/api/matches/{id}/source", params(("id"=Uuid, Path)), responses((status=200, body=String, content_type="application/x-go-sgf"), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn source(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Response> {
    let sgf = get_match_source_sgf(&state, id).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-go-sgf"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=match.sgf",
            ),
        ],
        sgf,
    )
        .into_response())
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct PositionQuery {
    pub database_id: Option<Uuid>,
    pub match_id: Option<Uuid>,
    pub game_no: Option<i32>,
    pub position_id: Option<String>,
    pub after: Option<Uuid>,
    pub limit: Option<i64>,
}

pub(crate) async fn list_positions(
    state: &AppState,
    query: PositionQuery,
) -> Result<PositionPage, DomainError> {
    if let Some(id) = query.match_id {
        crate::play::require_review_available(&state.pool, id).await?;
    }
    if query.game_no.is_some() {
        let match_id = query
            .match_id
            .ok_or_else(|| DomainError::invalid("game_no requires match_id"))?;
        validate_game(state, match_id, query.game_no).await?;
    }
    if let Some(ref id) = query.position_id
        && !valid_id(id, 14)
    {
        return Err(DomainError::invalid(
            "Expected a 14-character GNUbg Position ID",
        ));
    }
    if query.position_id.is_none() && query.match_id.is_none() {
        return Err(DomainError::invalid("Supply match_id or position_id"));
    }
    if let Some(cursor) = query.after {
        let p = store::get_position(&state.pool, cursor)
            .await?
            .ok_or_else(|| DomainError::invalid("Unknown cursor"))?;
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
                return Err(DomainError::invalid(
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
            return Err(DomainError::invalid("Cursor does not match the filters"));
        }
    }
    Ok(store::positions_scoped_in_database(
        &state.pool,
        query.match_id,
        query.position_id.as_deref(),
        query.after,
        limit(query.limit)?,
        query.game_no,
        query.database_id,
    )
    .await?)
}

#[utoipa::path(get, path="/api/positions", params(PositionQuery), responses((status=200, body=PositionPage), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn positions(
    State(state): State<AppState>,
    Query(query): Query<PositionQuery>,
) -> ApiResult<Json<PositionPage>> {
    Ok(Json(list_positions(&state, query).await?))
}

pub(crate) async fn get_match_dice(
    state: &AppState,
    id: Uuid,
    game_no: Option<i32>,
) -> Result<DiceStats, DomainError> {
    crate::play::require_review_available(&state.pool, id).await?;
    validate_game(state, id, game_no).await?;
    Ok(store::dice_stats_scoped(&state.pool, id, game_no).await?)
}

#[utoipa::path(get, path="/api/matches/{id}/dice", params(("id"=Uuid, Path), GameScope), responses((status=200, body=DiceStats), (status=404, body=ErrorBody), (status=422, body=ErrorBody)), tag="Library")]
pub(super) async fn dice(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<GameScope>,
) -> ApiResult<Json<DiceStats>> {
    Ok(Json(get_match_dice(&state, id, query.game_no).await?))
}
