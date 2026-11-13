//! User-initiated transfer to the local AnkiConnect instance.
use super::*;
use anyhow::{Context, ensure};
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
const MAX_RESPONSE_BYTES: usize = 32 * 1024;
static TRANSFER: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

#[derive(Deserialize)]
struct Reply {
    result: Option<bool>,
    error: Option<String>,
}

#[utoipa::path(post,path="/api/decks/{id}/send-to-anki",params(("id"=Uuid,Path)),request_body=ExportQuery,responses((status=204),(status=404,body=ErrorBody),(status=422,body=ErrorBody),(status=429,body=ErrorBody),(status=502,body=ErrorBody),(status=503,body=ErrorBody),(status=504,body=ErrorBody)),tag="Learning")]
pub(super) async fn send(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    LearningJson(query): LearningJson<ExportQuery>,
) -> ApiResult<StatusCode> {
    let _transfer = TRANSFER.try_acquire().map_err(|_| {
        ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "An Anki transfer is already in progress".into(),
        )
    })?;
    let bytes = build_deck_package(&state, id, query).await?;
    let directory = tempfile::tempdir().map_err(anyhow::Error::from)?;
    let package = directory.path().join("bg-tutor.apkg");
    tokio::fs::write(&package, bytes)
        .await
        .map_err(anyhow::Error::from)?;
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
    let key = std::env::var("BGTUTOR_ANKI_CONNECT_KEY").ok();
    let outcome =
        tokio::time::timeout(TIMEOUT, import_package(stream, &package, key.as_deref())).await;
    match outcome {
        Ok(Ok(())) => Ok(StatusCode::NO_CONTENT),
        Ok(Err(error)) => Err(ApiError(StatusCode::BAD_GATEWAY, error.to_string())),
        Err(_) => Err(ApiError(
            StatusCode::GATEWAY_TIMEOUT,
            "AnkiConnect did not confirm the import. Check the deck in Anki before retrying."
                .into(),
        )),
    }
}

async fn import_package(
    mut stream: TcpStream,
    path: &FilePath,
    key: Option<&str>,
) -> anyhow::Result<()> {
    let path = path
        .to_str()
        .context("Anki package path is not valid UTF-8")?;
    let mut body =
        serde_json::json!({"action":"importPackage", "version":6, "params":{"path":path}});
    if let Some(key) = key.filter(|key| !key.is_empty()) {
        body["key"] = serde_json::json!(key);
    }
    let body = serde_json::to_vec(&body)?;
    // AnkiConnect returns a content-length-delimited response and closes the socket.
    let header = format!(
        "POST / HTTP/1.1\r\nHost: {ADDRESS}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(&body).await?;
    let mut response = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let count = stream.read(&mut chunk).await?;
        ensure!(count > 0, "AnkiConnect returned an incomplete response");
        response.extend_from_slice(&chunk[..count]);
        ensure!(
            response.len() <= MAX_RESPONSE_BYTES,
            "AnkiConnect returned an oversized response"
        );
        if let Some(body) = response_body(&response)? {
            let reply: Reply = serde_json::from_slice(body)
                .context("AnkiConnect returned an unexpected response")?;
            if let Some(error) = reply.error {
                let error = if let Some(key) = key.filter(|key| !key.is_empty()) {
                    error.replace(key, "[redacted]")
                } else {
                    error
                };
                anyhow::bail!(
                    "AnkiConnect: {}",
                    error
                        .chars()
                        .filter(|c| !c.is_control())
                        .take(300)
                        .collect::<String>()
                );
            }
            ensure!(
                reply.result == Some(true),
                "AnkiConnect did not import the package"
            );
            return Ok(());
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
                        "action":"importPackage","version":6,"params":{"path":"/tmp/bg tutor.apkg"},"key":"test-key"
                    })
                );
                let header = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", reply.len());
                stream.get_mut().write_all(header.as_bytes()).await.unwrap();
                stream.get_mut().write_all(reply.as_bytes()).await.unwrap();
            });
            let stream = TcpStream::connect(address).await.unwrap();
            let result = import_package(
                stream,
                FilePath::new("/tmp/bg tutor.apkg"),
                Some("test-key"),
            )
            .await;
            assert_eq!(result.is_ok(), accepted, "{result:?}");
            server.await.unwrap();
        }
    }
}
