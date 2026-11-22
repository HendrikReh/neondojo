//! `Json` and `Query` extractors whose rejections are an [`ErrorBody`] with
//! HTTP 422, like every other API error. Axum's own extractors reject with
//! plain text. The type names match Axum's so that utoipa still infers request
//! bodies and query parameters from handler signatures.
//!
//! [`ErrorBody`]: crate::api::ErrorBody
use crate::api::ApiError;
use axum::{
    body::{Body, Bytes},
    extract::{FromRequest, FromRequestParts, OptionalFromRequest, Request},
};

pub(crate) struct Json<T>(pub T);
impl<S, T> FromRequest<S> for Json<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        <axum::Json<T> as FromRequest<S>>::from_request(request, state)
            .await
            .map(|axum::Json(value)| Self(value))
            .map_err(|error| ApiError::invalid(error.body_text()))
    }
}

/// An optional JSON body: an empty body or a literal `null` is `None`. Unlike
/// `Option<axum::Json<T>>`, which treats a request without `Content-Type` as
/// absent, it always requires a JSON `Content-Type`, so a cross-site form or `text/plain` POST cannot reach a
/// handler that takes one without a CORS preflight.
impl<S, T> OptionalFromRequest<S> for Json<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned,
{
    type Rejection = ApiError;
    async fn from_request(request: Request, state: &S) -> Result<Option<Self>, Self::Rejection> {
        let headers = request.headers().clone();
        let bytes = Bytes::from_request(request, state)
            .await
            .map_err(|error| ApiError::invalid(error.body_text()))?;
        let mut request = Request::new(Body::from(if bytes.is_empty() {
            Bytes::from_static(b"null")
        } else {
            bytes
        }));
        *request.headers_mut() = headers;
        <axum::Json<Option<T>> as FromRequest<S>>::from_request(request, state)
            .await
            .map(|axum::Json(value)| value.map(Self))
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
