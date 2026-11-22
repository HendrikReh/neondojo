use super::*;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/position-browser", get(search))
        .route(
            "/api/position-browser/analysis-readiness",
            axum::routing::post(analysis_readiness),
        )
        .route(
            "/api/position-filters",
            get(list_filters).post(create_filter),
        )
        .route(
            "/api/position-filters/{id}",
            axum::routing::put(update_filter).delete(delete_filter),
        )
}

pub(super) fn query<T>(value: Result<Query<T>, QueryRejection>) -> ApiResult<T> {
    value
        .map(|v| v.0)
        .map_err(|e| ApiError::invalid(e.body_text()))
}

pub(super) fn json<T>(value: Result<Json<T>, JsonRejection>) -> ApiResult<T> {
    value
        .map(|v| v.0)
        .map_err(|e| ApiError::invalid(e.body_text()))
}

pub(super) fn conflict(message: &str) -> ApiError {
    ApiError(StatusCode::CONFLICT, message.into())
}

#[utoipa::path(post,path="/api/position-browser/analysis-readiness",request_body=crate::learning::AddPositions,responses((status=200,body=PositionAnalysisReadiness),(status=422,body=ErrorBody)),tag="Positions")]
pub(super) async fn analysis_readiness(
    State(s): State<AppState>,
    value: Result<Json<crate::learning::AddPositions>, JsonRejection>,
) -> ApiResult<Json<PositionAnalysisReadiness>> {
    let mut ids = json(value)?.validated_ids()?;
    let mut seen = BTreeSet::new();
    ids.retain(|id| seen.insert(*id));
    let resolved = crate::learning::saved_position_cards(&s.pool, &ids).await?;
    let mut result = PositionAnalysisReadiness {
        analysis_required: 0,
        unavailable: 0,
        first_analysis_required: None,
    };
    for id in ids {
        match resolved.get(&id) {
            None => result.unavailable += 1,
            Some(None) => {
                result.analysis_required += 1;
                result.first_analysis_required.get_or_insert(id);
            }
            Some(Some(_)) => {}
        }
    }
    Ok(Json(result))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        search,
        analysis_readiness,
        list_filters,
        create_filter,
        update_filter,
        delete_filter
    ),
    components(schemas(
        PositionFilterDefinition,
        PositionBrowserSort,
        PositionBrowserDirection,
        PositionAnalysisState,
        PositionBrowserPage,
        PositionBrowserHit,
        PositionAnalysisReadiness,
        PositionBrowserNote,
        PositionDeckMembership,
        PositionNativeError,
        SavedPositionFilter,
        CreatePositionFilter,
        UpdatePositionFilter
    ))
)]
pub struct PositionBrowserDoc;
