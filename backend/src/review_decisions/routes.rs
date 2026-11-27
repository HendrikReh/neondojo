use super::*;

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/matches/{id}/review-decisions", get(review))
}

#[utoipa::path(get,path="/api/matches/{id}/review-decisions",params(("id"=Uuid,Path),ReviewQuery),responses((status=200,body=ReviewDecisionPage),(status=404,body=ErrorBody),(status=422,body=ErrorBody),(status=503,body=ErrorBody)),tag="Match analysis")]
pub(super) async fn review(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ReviewQuery>,
) -> ApiResult<Json<ReviewDecisionPage>> {
    Ok(Json(load_review(&state, id, query).await?))
}

#[derive(OpenApi)]
#[openapi(paths(review), components(schemas(ReviewDecision, ReviewDecisionPage)))]
pub struct ReviewDoc;
