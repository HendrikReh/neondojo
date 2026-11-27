//! Bounded URL-safe Base64 JSON pagination cursors.
use crate::api::{ApiError, ApiResult};
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Serialize, de::DeserializeOwned};

pub(crate) const DEFAULT_MAX_LEN: usize = 4096;

pub(crate) fn encode<T: Serialize>(value: &T) -> Result<String> {
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(value)?))
}

pub(crate) fn decode<T: DeserializeOwned>(
    raw: &str,
    max_len: usize,
    invalid_message: &str,
) -> ApiResult<T> {
    if raw.len() > max_len {
        return Err(ApiError::invalid(invalid_message));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| ApiError::invalid(invalid_message))?;
    serde_json::from_slice(&bytes).map_err(|_| ApiError::invalid(invalid_message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct SampleCursor {
        page: u32,
        tag: String,
    }

    #[test]
    fn round_trips_and_enforces_max_len() {
        let value = SampleCursor {
            page: 3,
            tag: "ok".into(),
        };
        let encoded = encode(&value).unwrap();
        let decoded: SampleCursor = decode(&encoded, DEFAULT_MAX_LEN, "bad cursor").unwrap();
        assert_eq!(decoded, value);

        let err = decode::<SampleCursor>(&encoded, encoded.len() - 1, "too long").unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.1.error, "too long");

        let err = decode::<SampleCursor>("!!!not-base64!!!", 64, "bad b64").unwrap_err();
        assert_eq!(err.1.error, "bad b64");

        let invalid_json = URL_SAFE_NO_PAD.encode(b"{not json");
        let err = decode::<SampleCursor>(&invalid_json, 64, "bad json").unwrap_err();
        assert_eq!(err.1.error, "bad json");
    }
}
