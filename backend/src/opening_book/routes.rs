use super::*;

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

#[utoipa::path(get,path="/api/opening-book",responses((status=200,body=BookSettings)),tag="Opening book")]
pub(super) async fn get_settings(State(state): State<AppState>) -> ApiResult<Json<BookSettings>> {
    let config = load(&state.pool).await?;
    let status = status(&config).await;
    let xg_status = crate::xg_book::status(&config.xg_path).await;
    Ok(Json(BookSettings {
        config,
        status,
        xg_status,
    }))
}

#[utoipa::path(put,path="/api/opening-book",request_body=BookConfigWrite,responses((status=200,body=BookSettings),(status=409,body=ErrorBody),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn put_settings(
    State(state): State<AppState>,
    payload: Result<Json<BookConfigWrite>, JsonRejection>,
) -> ApiResult<Json<BookSettings>> {
    let Json(config) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    if !(0..i64::MAX).contains(&config.version)
        || !matches!(
            config.provider.as_str(),
            "off" | "backgammondb" | "automatic" | "xg"
        )
    {
        return Err(ApiError::invalid(
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
    let saved:Option<BookConfig>=sqlx::query_as("UPDATE opening_book_settings SET version=version+1,provider=$2,path=$3,xg_path=coalesce($4,xg_path) WHERE singleton=true AND version=$1 RETURNING version,provider,path,xg_path")
        .bind(config.version).bind(config.provider).bind(config.path).bind(config.xg_path).fetch_optional(&mut *tx).await.map_err(anyhow::Error::from)?;
    let config = saved.ok_or_else(|| {
        ApiError(
            StatusCode::CONFLICT,
            "Opening book settings changed; reload before saving".into(),
        )
    })?;
    crate::configuration::store::legacy_book(&mut tx, &config)
        .await
        .map_err(crate::configuration::store::legacy_error)?;
    tx.commit().await.map_err(anyhow::Error::from)?;
    let status = status(&config).await;
    let xg_status = crate::xg_book::status(&config.xg_path).await;
    Ok(Json(BookSettings {
        config,
        status,
        xg_status,
    }))
}

#[utoipa::path(post,path="/api/opening-book/check",request_body=BookPathRequest,responses((status=200,body=BookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn check_book(
    payload: Result<Json<BookPathRequest>, JsonRejection>,
) -> ApiResult<Json<BookStatus>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    validate_path(&request.path)?;
    Ok(Json(
        status(&BookConfig {
            version: 0,
            provider: "backgammondb".into(),
            path: request.path,
            xg_path: crate::xg_book::default_path(),
        })
        .await,
    ))
}

#[utoipa::path(post,path="/api/opening-book/install",request_body=BookPathRequest,responses((status=200,body=BookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn install_book(
    State(state): State<AppState>,
    payload: Result<Json<BookPathRequest>, JsonRejection>,
) -> ApiResult<Json<BookStatus>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    validate_path(&request.path)?;
    let config = load(&state.pool).await?;
    Ok(Json(
        install_from_path(&config, &request.path)
            .await
            .map_err(|error| {
                ApiError::invalid(format!("Cannot install opening book: {error:#}"))
            })?,
    ))
}

#[utoipa::path(post,path="/api/opening-book/xg/check",request_body=BookPathRequest,responses((status=200,body=crate::xg_book::XgBookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn check_xg_book(
    payload: Result<Json<BookPathRequest>, JsonRejection>,
) -> ApiResult<Json<crate::xg_book::XgBookStatus>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    validate_path(&request.path)?;
    Ok(Json(crate::xg_book::status(&request.path).await))
}

#[utoipa::path(post,path="/api/opening-book/xg/install",request_body=BookPathRequest,responses((status=200,body=crate::xg_book::XgBookStatus),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn install_xg_book(
    State(state): State<AppState>,
    payload: Result<Json<BookPathRequest>, JsonRejection>,
) -> ApiResult<Json<crate::xg_book::XgBookStatus>> {
    let Json(request) = payload.map_err(|error| ApiError::invalid(error.body_text()))?;
    validate_path(&request.path)?;
    let config = load(&state.pool).await?;
    Ok(Json(
        crate::xg_book::install_from_path(&config.xg_path, &request.path)
            .await
            .map_err(|error| {
                ApiError::invalid(format!("Cannot install XG opening book: {error:#}"))
            })?,
    ))
}

#[utoipa::path(get,path="/api/positions/{id}/opening-book",params(("id"=Uuid,Path)),responses((status=200,body=BookLookup),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Opening book")]
pub(super) async fn position_book(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<BookLookup>> {
    crate::play::require_position_review_available(&state.pool, id).await?;
    let position = crate::store::get_position(&state.pool, id)
        .await?
        .ok_or_else(ApiError::missing)?;
    let config = load(&state.pool).await?;
    let beaver = source_beaver_rule(&state.pool, &position).await?;
    Ok(Json(
        lookup_with_rules(&config, &position, None, beaver).await?,
    ))
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
        crate::xg_book::XgBookStatus,
        crate::xg_book::XgPositionResult,
        crate::xg_book::XgEvaluation,
        crate::xg_book::XgRollout
    ))
)]
pub struct OpeningBookDoc;
