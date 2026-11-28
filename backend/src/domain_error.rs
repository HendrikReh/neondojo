//! Transport-agnostic domain errors shared by the in-process [`crate::facade::BackendFacade`]
//! and the HTTP [`crate::api::ApiError`] adapter.

use crate::api::{ApiError, ErrorBody};
use axum::http::StatusCode;
use std::fmt;

/// Semantic error category independent of HTTP transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    BadRequest,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    RequestTimeout,
    Conflict,
    PayloadTooLarge,
    UnsupportedMediaType,
    UnprocessableEntity,
    TooManyRequests,
    ServiceUnavailable,
    GatewayTimeout,
    Internal,
}

impl ErrorKind {
    pub fn from_status(status: StatusCode) -> Self {
        match status {
            StatusCode::BAD_REQUEST => Self::BadRequest,
            StatusCode::FORBIDDEN => Self::Forbidden,
            StatusCode::NOT_FOUND => Self::NotFound,
            StatusCode::METHOD_NOT_ALLOWED => Self::MethodNotAllowed,
            StatusCode::REQUEST_TIMEOUT => Self::RequestTimeout,
            StatusCode::CONFLICT => Self::Conflict,
            StatusCode::PAYLOAD_TOO_LARGE => Self::PayloadTooLarge,
            StatusCode::UNSUPPORTED_MEDIA_TYPE => Self::UnsupportedMediaType,
            StatusCode::UNPROCESSABLE_ENTITY => Self::UnprocessableEntity,
            StatusCode::TOO_MANY_REQUESTS => Self::TooManyRequests,
            StatusCode::SERVICE_UNAVAILABLE => Self::ServiceUnavailable,
            StatusCode::GATEWAY_TIMEOUT => Self::GatewayTimeout,
            _ => Self::Internal,
        }
    }

    pub fn status_code(self) -> StatusCode {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::RequestTimeout => StatusCode::REQUEST_TIMEOUT,
            Self::Conflict => StatusCode::CONFLICT,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::UnsupportedMediaType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::UnprocessableEntity => StatusCode::UNPROCESSABLE_ENTITY,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
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

    pub fn too_many_requests(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::TooManyRequests, message.into())
    }

    pub fn payload_too_large(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::PayloadTooLarge, message.into())
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

impl From<crate::configuration::ConfigurationError> for DomainError {
    fn from(error: crate::configuration::ConfigurationError) -> Self {
        ApiError::from(error).into()
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
            (ErrorKind::Forbidden, StatusCode::FORBIDDEN),
            (ErrorKind::NotFound, StatusCode::NOT_FOUND),
            (ErrorKind::MethodNotAllowed, StatusCode::METHOD_NOT_ALLOWED),
            (ErrorKind::RequestTimeout, StatusCode::REQUEST_TIMEOUT),
            (ErrorKind::Conflict, StatusCode::CONFLICT),
            (ErrorKind::PayloadTooLarge, StatusCode::PAYLOAD_TOO_LARGE),
            (
                ErrorKind::UnsupportedMediaType,
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
            ),
            (
                ErrorKind::UnprocessableEntity,
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (ErrorKind::TooManyRequests, StatusCode::TOO_MANY_REQUESTS),
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
