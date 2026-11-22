//! `Json` and `Query` extractors whose rejections are an [`ErrorBody`] with
//! HTTP 422, like every other API error. Axum's own extractors reject with
//! plain text. The type names match Axum's so that utoipa still infers request
//! bodies and query parameters from handler signatures.
//!
//! [`ErrorBody`]: crate::api::ErrorBody
use crate::api::ApiError;
use axum::extract::{FromRequest, FromRequestParts, Request};

pub(crate) struct Json<T>(pub T);
impl<S, T> FromRequest<S> for Json<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        axum::Json::<T>::from_request(request, state)
            .await
            .map(|axum::Json(value)| Self(value))
            .map_err(|error| ApiError::invalid(error.body_text()))
    }
}

pub(crate) struct Query<T>(pub T);
impl<S, T> FromRequestParts<S> for Query<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Query(value)| Self(value))
            .map_err(|error| ApiError::invalid(error.body_text()))
    }
}
