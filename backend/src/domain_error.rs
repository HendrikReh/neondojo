//! Transport-agnostic domain errors shared by the in-process [`crate::facade::BackendFacade`]
//! and the HTTP [`crate::api::ApiError`] adapter.

use crate::api::{ApiError, ErrorBody};
use axum::http::StatusCode;
use std::fmt;

/// Semantic error category independent of HTTP transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    BadRequest,
    NotFound,
    MethodNotAllowed,
    Conflict,
    UnprocessableEntity,
    ServiceUnavailable,
    GatewayTimeout,
    Internal,
}

impl ErrorKind {
    pub fn from_status(status: StatusCode) -> Self {
        match status {
            StatusCode::BAD_REQUEST => Self::BadRequest,
            StatusCode::NOT_FOUND => Self::NotFound,
            StatusCode::METHOD_NOT_ALLOWED => Self::MethodNotAllowed,
            StatusCode::CONFLICT => Self::Conflict,
            StatusCode::UNPROCESSABLE_ENTITY => Self::UnprocessableEntity,
            StatusCode::SERVICE_UNAVAILABLE => Self::ServiceUnavailable,
            StatusCode::GATEWAY_TIMEOUT => Self::GatewayTimeout,
            _ => Self::Internal,
        }
    }

    pub fn status_code(self) -> StatusCode {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::Conflict => StatusCode::CONFLICT,
            Self::UnprocessableEntity => StatusCode::UNPROCESSABLE_ENTITY,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::GatewayTimeout => StatusCode::GATEWAY_TIMEOUT,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Structured error returned by transport-agnostic backend services.
#[derive(Debug)]
pub struct DomainError {
    pub kind: ErrorKind,
    pub body: ErrorBody,
}

impl DomainError {
    pub fn new(kind: ErrorKind, body: impl Into<ErrorBody>) -> Self {
        Self {
            kind,
            body: body.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::UnprocessableEntity, message.into())
    }

    pub fn missing() -> Self {
        Self::new(ErrorKind::NotFound, "Not found")
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Conflict, message.into())
    }

    pub fn restore_in_progress() -> Self {
        Self::new(
            ErrorKind::ServiceUnavailable,
            "Database restoration is in progress",
        )
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::ServiceUnavailable, message.into())
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(code) = &self.body.code {
            write!(f, "[{code}] {}", self.body.error)
        } else {
            f.write_str(&self.body.error)
        }
    }
}

impl std::error::Error for DomainError {}

impl From<DomainError> for ApiError {
    fn from(error: DomainError) -> Self {
        ApiError(error.kind.status_code(), error.body)
    }
}

impl From<ApiError> for DomainError {
    fn from(error: ApiError) -> Self {
        Self {
            kind: ErrorKind::from_status(error.0),
            body: error.1,
        }
    }
}

impl From<anyhow::Error> for DomainError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(error = ?error, "Service call failed");
        Self::new(
            ErrorKind::Internal,
            "Internal server error; see backend log",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_status_codes_and_structured_body() {
        let cases = [
            (ErrorKind::BadRequest, StatusCode::BAD_REQUEST),
            (ErrorKind::NotFound, StatusCode::NOT_FOUND),
            (ErrorKind::MethodNotAllowed, StatusCode::METHOD_NOT_ALLOWED),
            (ErrorKind::Conflict, StatusCode::CONFLICT),
            (
                ErrorKind::UnprocessableEntity,
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                ErrorKind::ServiceUnavailable,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (ErrorKind::GatewayTimeout, StatusCode::GATEWAY_TIMEOUT),
            (ErrorKind::Internal, StatusCode::INTERNAL_SERVER_ERROR),
        ];
        for (kind, status) in cases {
            assert_eq!(kind.status_code(), status);
            assert_eq!(ErrorKind::from_status(status), kind);
        }

        let err = DomainError {
            kind: ErrorKind::Conflict,
            body: ErrorBody {
                error: "stale revision".into(),
                code: Some("revision_conflict".into()),
                issues: None,
            },
        };
        assert_eq!(err.to_string(), "[revision_conflict] stale revision");
        let api_err: ApiError = err.into();
        assert_eq!(api_err.0, StatusCode::CONFLICT);
        assert_eq!(api_err.1.code.as_deref(), Some("revision_conflict"));
        let back: DomainError = api_err.into();
        assert_eq!(back.kind, ErrorKind::Conflict);
        assert_eq!(back.body.error, "stale revision");
    }
}
