use super::*;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/matches/{id}/review-decisions", get(review))
}

#[utoipa::path(get,path="/api/matches/{id}/review-decisions",params(("id"=Uuid,Path),ReviewQuery),responses((status=200,body=ReviewDecisionPage),(status=404,body=ErrorBody),(status=422,body=ErrorBody)),tag="Match analysis")]
pub(super) async fn review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    query: Result<Query<ReviewQuery>, axum::extract::rejection::QueryRejection>,
) -> ApiResult<Json<ReviewDecisionPage>> {
    let Query(query) = query.map_err(|e| ApiError::invalid(e.body_text()))?;
    Ok(Json(load_review(&state, id, query).await?))
}

#[derive(OpenApi)]
#[openapi(paths(review), components(schemas(ReviewDecision, ReviewDecisionPage)))]
pub struct ReviewDoc;
