//! Extractors whose rejections are an [`ErrorBody`], like every other API
//! error. Axum's own extractors reject with plain text, so `clippy.toml` bans
//! them outside this module. The type names match Axum's so that utoipa still
//! infers request bodies and query parameters from handler signatures.
//!
//! A malformed body or query is HTTP 422. A malformed path and a request that
//! is not multipart keep Axum's status, normally HTTP 400.
//!
//! A handler that has to report something else first, such as a missing
//! record, takes `Result<Json<T>, ApiError>` and unwraps it later.
//!
//! [`ErrorBody`]: crate::api::ErrorBody
#![allow(clippy::disallowed_types)]
use crate::api::ApiError;
use axum::{
    body::{Body, Bytes},
    extract::{FromRequest, FromRequestParts, OptionalFromRequest, Request},
    http::request::Parts,
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};

/// A JSON request body, or a JSON response.
#[derive(Debug)]
pub struct Json<T>(pub T);
impl<S, T> FromRequest<S> for Json<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
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
    T: DeserializeOwned,
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

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

pub struct Query<T>(pub T);
impl<S, T> FromRequestParts<S> for Query<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Query(value)| Self(value))
            .map_err(|error| ApiError::invalid(error.body_text()))
    }
}

pub struct Path<T>(pub T);
impl<S, T> FromRequestParts<S> for Path<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Path::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Path(value)| Self(value))
            .map_err(|error| ApiError(error.status(), error.body_text().into()))
    }
}

pub struct Multipart(pub axum::extract::Multipart);
impl<S: Send + Sync> FromRequest<S> for Multipart {
    type Rejection = ApiError;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        <axum::extract::Multipart as FromRequest<S>>::from_request(request, state)
            .await
            .map(Self)
            .map_err(|error| ApiError(error.status(), error.body_text().into()))
    }
}
