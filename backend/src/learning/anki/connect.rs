//! User-initiated transfer to the local AnkiConnect instance.
use super::{super::*, deck_export::build_deck_package};
use anyhow::{Context, ensure};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::Path as FilePath,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

const ADDRESS: SocketAddr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST), 8765);
const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub(super) fn reserve(state: &AppState) -> ApiResult<tokio::sync::OwnedSemaphorePermit> {
    state
        .anki_limits
        .connect
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "An Anki operation is already in progress".into(),
            )
        })
}

pub(super) struct Client {
    key: Option<String>,
}

impl Client {
    pub(super) fn local() -> Self {
        Self {
            key: std::env::var("NEONDOJO_ANKI_CONNECT_KEY")
                .ok()
                .filter(|key| !key.is_empty()),
        }
    }

    pub(super) async fn rpc<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> ApiResult<T> {
        let body = json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params});
        let reply = self.request("/api/v1", &body, self.key.as_deref()).await?;
        if reply["id"] != 1 {
            return Err(upstream_error(
                "AnkiConnect must support its versioned API to synchronize reviews",
            ));
        }
        if let Some(error) = reply["error"]["message"].as_str() {
            match reply["error"]["data"]["type"].as_str() {
                Some("collectionUnavailable") => {
                    return Err(ApiError(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Open an Anki profile".into(),
                    ));
                }
                Some("staleSession" | "editorConflict" | "conflict" | "busy") => {
                    return Err(ApiError(
                        StatusCode::CONFLICT,
                        sanitize(error, self.key.as_deref()).into(),
                    ));
                }
                _ => {}
            }
            return Err(self.error(error));
        }
        serde_json::from_value(reply["result"].clone())
            .map_err(|_| upstream_error("AnkiConnect returned unexpected data"))
    }

    pub(super) fn error(&self, error: &str) -> ApiError {
        upstream_error(sanitize(error, self.key.as_deref()))
    }

    async fn request(&self, path: &str, body: &Value, bearer: Option<&str>) -> ApiResult<Value> {
        let stream = tokio::time::timeout(Duration::from_secs(2), TcpStream::connect(ADDRESS))
            .await
            .ok()
            .and_then(Result::ok)
            .ok_or_else(|| {
                ApiError(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Open Anki Desktop with AnkiConnect listening on port 8765".into(),
                )
            })?;
        match tokio::time::timeout(TIMEOUT, exchange(stream, path, body, bearer)).await {
            Ok(result) => result.map_err(|error| self.error(&error.to_string())),
            Err(_) => Err(ApiError(
                StatusCode::GATEWAY_TIMEOUT,
                "AnkiConnect did not confirm the operation in time".into(),
            )),
        }
    }
}

pub(super) fn upstream_error(message: impl Into<ErrorBody>) -> ApiError {
    ApiError(StatusCode::BAD_GATEWAY, message.into())
}

fn sanitize(error: &str, key: Option<&str>) -> String {
    let error = match key.filter(|key| !key.is_empty()) {
        Some(key) => error.replace(key, "[redacted]"),
        None => error.to_owned(),
    };
    error
        .chars()
        .filter(|c| !c.is_control())
        .take(300)
        .collect()
}

pub(crate) async fn send_deck_service(
    state: &AppState,
    id: Uuid,
    query: ExportQuery,
) -> Result<(), crate::domain_error::DomainError> {
    let _transfer = reserve(state)?;
    let bytes = build_deck_package(state, id, query).await?;
    let directory = crate::async_temp_dir::AsyncTempDir::new().await?;
    let package = directory.path().join("neondojo.apkg");
    tokio::fs::write(&package, bytes)
        .await
        .map_err(anyhow::Error::from)?;
    let client = Client::local();
    let body = package_request(&package, client.key.as_deref())?;
    let reply = client.request("/", &body, None).await?;
    confirm_import(&reply, client.key.as_deref())
        .map_err(|error| upstream_error(error.to_string()))?;
    Ok(())
}

#[utoipa::path(post,path="/api/decks/{id}/send-to-anki",params(("id"=Uuid,Path)),request_body=ExportQuery,responses((status=204),(status=404,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=502,body=ErrorBody),(status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(in crate::learning) async fn send(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(query): Json<ExportQuery>,
) -> ApiResult<StatusCode> {
    send_deck_service(&state, id, query).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn package_request(path: &FilePath, key: Option<&str>) -> anyhow::Result<Value> {
    let path = path
        .to_str()
        .context("Anki package path is not valid UTF-8")?;
    let mut body = json!({"action":"importPackage", "version":6, "params":{"path":path}});
    if let Some(key) = key {
        body["key"] = json!(key);
    }
    Ok(body)
}

fn confirm_import(reply: &Value, key: Option<&str>) -> anyhow::Result<()> {
    if let Some(error) = reply["error"].as_str() {
        anyhow::bail!("AnkiConnect: {}", sanitize(error, key));
    }
    ensure!(
        reply["result"] == true,
        "AnkiConnect did not import the package"
    );
    Ok(())
}

async fn exchange(
    mut stream: TcpStream,
    path: &str,
    body: &Value,
    bearer: Option<&str>,
) -> anyhow::Result<Value> {
    let body = serde_json::to_vec(body)?;
    let mut header = format!(
        "POST {path} HTTP/1.1\r\nHost: {ADDRESS}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        body.len()
    );
    if let Some(key) = bearer {
        ensure!(
            !key.chars().any(char::is_control),
            "Invalid AnkiConnect API key"
        );
        header.push_str(&format!("Authorization: Bearer {key}\r\n"));
    }
    header.push_str("\r\n");
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&body).await?;
    let mut response = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let count = stream.read(&mut chunk).await?;
        ensure!(count > 0, "AnkiConnect returned an incomplete response");
        response.extend_from_slice(&chunk[..count]);
        ensure!(
            response.len() <= MAX_RESPONSE_BYTES,
            "AnkiConnect returned an oversized response"
        );
        if let Some(body) = response_body(&response)? {
            return serde_json::from_slice(body)
                .context("AnkiConnect returned an unexpected response");
        }
    }
}

fn response_body(response: &[u8]) -> anyhow::Result<Option<&[u8]>> {
    let Some(end) = response.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Ok(None);
    };
    let header = std::str::from_utf8(&response[..end])
        .context("AnkiConnect returned invalid HTTP headers")?;
    let mut lines = header.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .context("AnkiConnect returned an invalid HTTP status")?;
    ensure!(
        (200..300).contains(&status),
        "AnkiConnect returned HTTP {status}"
    );
    let length: usize = lines
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("Content-Length"))
        .and_then(|(_, length)| length.trim().parse().ok())
        .context("AnkiConnect did not report a response length")?;
    ensure!(
        length <= MAX_RESPONSE_BYTES,
        "AnkiConnect returned an oversized response"
    );
    let body = &response[end + 4..];
    Ok(body.get(..length))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncBufReadExt, BufReader},
        net::TcpListener,
    };

    #[tokio::test]
    async fn sends_a_package_path_and_requires_import_confirmation() {
        for (reply, accepted) in [
            (r#"{"result":true,"error":null}"#, true),
            (r#"{"result":false,"error":null}"#, false),
            (r#"{"result":null,"error":"unsupported action"}"#, false),
        ] {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut stream = BufReader::new(stream);
                let mut length = None;
                loop {
                    let mut line = String::new();
                    assert!(stream.read_line(&mut line).await.unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.strip_prefix("Content-Length:") {
                        length = Some(value.trim().parse::<usize>().unwrap());
                    }
                }
                let mut body = vec![0; length.unwrap()];
                stream.read_exact(&mut body).await.unwrap();
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
                    serde_json::json!({
                        "action":"importPackage","version":6,"params":{"path":"/tmp/neon dojo.apkg"},"key":"test-key"
                    })
                );
                let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", reply.len());
                stream.get_mut().write_all(header.as_bytes()).await.unwrap();
                stream.get_mut().write_all(reply.as_bytes()).await.unwrap();
            });
            let stream = TcpStream::connect(address).await.unwrap();
            let key = Some("test-key");
            let body = package_request(FilePath::new("/tmp/neon dojo.apkg"), key).unwrap();
            let result = confirm_import(&exchange(stream, "/", &body, None).await.unwrap(), key);
            assert_eq!(result.is_ok(), accepted, "{result:?}");
            server.await.unwrap();
        }
    }
}
