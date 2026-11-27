use super::*;
use crate::domain_error::DomainError;

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/opening-book", get(get_settings).put(put_settings))
        .route("/api/opening-book/check", post(check_book))
        .route("/api/opening-book/install", post(install_book))
        .route("/api/opening-book/xg/check", post(check_xg_book))
        .route("/api/opening-book/xg/install", post(install_xg_book))
        .route("/api/positions/{id}/opening-book", get(position_book))
}

pub(super) fn validate_path(path: &str) -> ApiResult<()> {
    if path.is_empty()
        || path.len() > 4096
        || path.chars().any(char::is_control)
        || !FsPath::new(path).is_absolute()
    {
        return Err(ApiError::invalid(
            "Opening book path must be absolute, nonempty, at most 4096 bytes and contain no control characters",
        ));
    }
    Ok(())
}

pub(crate) async fn read_settings(state: &AppState) -> Result<BookSettings, DomainError> {
    let config = load(&state.pool).await?;
    let status = status(&config).await;
    let xg_status = state.opening_books.xg.status(&config.xg_path).await;
    Ok(BookSettings {
        config,
        status,
        xg_status,
    })
}

pub(crate) async fn write_settings(
    state: &AppState,
    config: BookConfigWrite,
) -> Result<BookSettings, DomainError> {
    if !(0..i64::MAX).contains(&config.version)
        || !matches!(
            config.provider.as_str(),
            "off" | "backgammondb" | "automatic" | "xg"
        )
    {
        return Err(DomainError::invalid(
            "Invalid opening book version or provider",
        ));
    }
    validate_path(&config.path)?;
    if let Some(path) = &config.xg_path {
        validate_path(path)?;
    }
    load(&state.pool).await?;
    let mut tx = state.pool.begin().await.map_err(anyhow::Error::from)?;
    crate::configuration::store::lock(&mut tx)
        .await
        .map_err(anyhow::Error::from)?;
    let latest: Option<BookConfig> = sqlx::query_as(
        "SELECT version,provider,path,xg_path FROM opening_book_settings WHERE singleton=true FOR UPDATE",
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    let latest = latest
        .filter(|latest| latest.version == config.version)
        .ok_or_else(|| {
            DomainError::conflict("Opening book settings changed; reload before saving")
        })?;
    let config = BookConfig {
        version: config.version,
        provider: config.provider,
        path: config.path,
        xg_path: config.xg_path.unwrap_or(latest.xg_path),
    };
    crate::configuration::store::save_book(&mut tx, &config).await?;
    // One save is one revision, whether or not it changed a value.
    let config: BookConfig = sqlx::query_as(
        "UPDATE opening_book_settings SET version=$1+1 WHERE singleton=true RETURNING version,provider,path,xg_path",
    )
    .bind(config.version)
    .fetch_one(&mut *tx)
    .await
    .map_err(anyhow::Error::from)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    let status = status(&config).await;
    let xg_status = state.opening_books.xg.status(&config.xg_path).await;
    Ok(BookSettings {
        config,
        status,
        xg_status,
    })
}

pub(crate) async fn check_book_path(path: String) -> Result<BookStatus, DomainError> {
    validate_path(&path)?;
    Ok(status(&BookConfig {
        version: 0,
        provider: "backgammondb".into(),
        path,
        xg_path: xg_book::default_path(),
    })
    .await)
}

pub(crate) async fn install_book_path(
    state: &AppState,
    path: String,
) -> Result<BookStatus, DomainError> {
    validate_path(&path)?;
    let config = load(&state.pool).await?;
    install_from_path(&config, &path)
        .await
        .map_err(|error| DomainError::invalid(format!("Cannot install opening book: {error:#}")))
}

pub(crate) async fn check_xg_book_path(
    state: &AppState,
    path: String,
) -> Result<xg_book::XgBookStatus, DomainError> {
    validate_path(&path)?;
    Ok(state.opening_books.xg.status(&path).await)
}

pub(crate) async fn install_xg_book_path(
    state: &AppState,
    path: String,
) -> Result<xg_book::XgBookStatus, DomainError> {
    validate_path(&path)?;
    let config = load(&state.pool).await?;
    state
        .opening_books
        .xg
        .install_from_path(&config.xg_path, &path)
        .await
        .map_err(|error| DomainError::invalid(format!("Cannot install XG opening book: {error:#}")))
}

#[utoipa::path(get,path="/api/opening-book",responses((status=200,body=BookSettings)),tag="Opening book")]
pub(super) async fn get_settings(State(state): State<AppState>) -> ApiResult<Json<BookSettings>> {
    Ok(Json(read_settings(&state).await?))
}

#[utoipa::path(put,path="/api/opening-book",request_body=BookConfigWrite,responses((status=200,body=BookSettings),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn put_settings(
    State(state): State<AppState>,
    Json(config): Json<BookConfigWrite>,
) -> ApiResult<Json<BookSettings>> {
    Ok(Json(write_settings(&state, config).await?))
}

#[utoipa::path(post,path="/api/opening-book/check",request_body=BookPathRequest,responses((status=200,body=BookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn check_book(
    Json(request): Json<BookPathRequest>,
) -> ApiResult<Json<BookStatus>> {
    Ok(Json(check_book_path(request.path).await?))
}

#[utoipa::path(post,path="/api/opening-book/install",request_body=BookPathRequest,responses((status=200,body=BookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn install_book(
    State(state): State<AppState>,
    Json(request): Json<BookPathRequest>,
) -> ApiResult<Json<BookStatus>> {
    Ok(Json(install_book_path(&state, request.path).await?))
}

#[utoipa::path(post,path="/api/opening-book/xg/check",request_body=BookPathRequest,responses((status=200,body=xg_book::XgBookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn check_xg_book(
    State(state): State<AppState>,
    Json(request): Json<BookPathRequest>,
) -> ApiResult<Json<xg_book::XgBookStatus>> {
    Ok(Json(check_xg_book_path(&state, request.path).await?))
}

#[utoipa::path(post,path="/api/opening-book/xg/install",request_body=BookPathRequest,responses((status=200,body=xg_book::XgBookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn install_xg_book(
    State(state): State<AppState>,
    Json(request): Json<BookPathRequest>,
) -> ApiResult<Json<xg_book::XgBookStatus>> {
    Ok(Json(install_xg_book_path(&state, request.path).await?))
}

pub(crate) async fn lookup_position_book(
    state: &AppState,
    id: Uuid,
) -> Result<BookLookup, DomainError> {
    crate::play::require_position_review_available(&state.pool, id).await?;
    let position = crate::store::get_position(&state.pool, id)
        .await?
        .ok_or_else(DomainError::missing)?;
    let config = load(&state.pool).await?;
    let beaver = state
        .opening_books
        .source_beaver_rule(&state.pool, &position)
        .await?;
    Ok(state
        .opening_books
        .lookup(&config, &position, None, beaver)
        .await?)
}

#[utoipa::path(get,path="/api/positions/{id}/opening-book",params(("id"=Uuid,Path)),responses((status=200,body=BookLookup),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn position_book(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BookLookup>> {
    Ok(Json(lookup_position_book(&state, id).await?))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_settings,
        put_settings,
        check_book,
        install_book,
        check_xg_book,
        install_xg_book,
        position_book
    ),
    components(schemas(
        BookConfig,
        BookConfigWrite,
        BookContext,
        BookStatus,
        BookSettings,
        BookPathRequest,
        BookLookup,
        BookChecker,
        BookCube,
        BookCubeEquities,
        BookProvenance,
        xg_book::XgBookStatus,
        xg_book::XgPositionResult,
        xg_book::XgEvaluation,
        xg_book::XgRollout
    ))
)]
pub struct OpeningBookDoc;
