//! The browser never receives the API key or the upstream response body.
use super::{Assessment, TopicName};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use utoipa::PartialSchema;

const DEFAULT_MODEL: &str = "gpt-6.1-terra";
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const HTTP_STATUS_BYTES: usize = 4;

fn key() -> Option<String> {
    std::env::var("OPENAI_API_KEY").ok().filter(|value| {
        !value.is_empty()
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    })
}

pub(super) fn configured() -> bool {
    key().is_some()
}

pub(super) fn model() -> String {
    std::env::var("NEONDOJO_ASSESSMENT_MODEL")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.into())
}

fn request(model: &str, input: &Value) -> Value {
    let string = json!({"type":"string"});
    let object = |properties: Value| {
        let required = properties
            .as_object()
            .map(|map| map.keys().cloned().collect::<Vec<_>>());
        json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
    };
    let list = |items| json!({"type":"array","items":items});
    json!({
        "model": model,
        "reasoning": {"effort":"low"},
        "store": false,
        "max_output_tokens": 5000,
        "instructions": include_str!("prompt.txt"),
        "input": input.to_string(),
        "text": {"format":{
            "type":"json_schema","name":"neondojo_match_assessment","strict":true,
            "schema": object(json!({
                "summary": string,
                "next_match_focus": string,
                "topics": list(object(json!({
                    "topic": TopicName::schema(), "observation":string,"evidence_refs":list(string.clone())
                }))),
                "key_decisions": list(object(json!({
                    "evidence_ref":string,"explanation":string,"next_question":string
                }))),
                "training": list(object(json!({
                    "focus":string,"exercise":string,"success_criterion":string,"evidence_refs":list(string.clone())
                }))),
                "limitations":list(string.clone())
            }))
        }}
    })
}

pub(super) async fn generate(model: &str, input: &Value) -> Result<Assessment, &'static str> {
    let secret = key().ok_or("not_configured")?;
    let body = serde_json::to_string(&request(model, input)).map_err(|_| "invalid_report")?;
    // Pass the header and body via stdin, away from process arguments and logs.
    // Ignore curlrc, enforce TLS, and do not follow redirects to other hosts.
    let config = format!(
        "header = \"Authorization: Bearer {secret}\"\ndata-binary = {}\n",
        serde_json::to_string(&body).map_err(|_| "invalid_report")?
    );
    tokio::time::timeout(Duration::from_secs(88), send(config))
        .await
        .map_err(|_| {
            tracing::warn!("OpenAI assessment exceeded the request deadline");
            "provider"
        })?
}

async fn send(config: String) -> Result<Assessment, &'static str> {
    let mut child = tokio::process::Command::new("/usr/bin/curl")
        .args([
            "-q",
            "--silent",
            "--fail-with-body",
            "--proto",
            "=https",
            "--max-time",
            "85",
            "--connect-timeout",
            "10",
            "--max-filesize",
            "262144",
            "--header",
            "Content-Type: application/json",
            "--config",
            "-",
            "--write-out",
            "\n%{http_code}",
            "https://api.openai.com/v1/responses",
        ])
        .env_remove("OPENAI_API_KEY")
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(curl_io_failure)?;
    let mut stdin = child.stdin.take().ok_or("provider")?;
    stdin
        .write_all(config.as_bytes())
        .await
        .map_err(curl_io_failure)?;
    drop(stdin);
    let mut raw = Vec::new();
    let output_limit = MAX_RESPONSE_BYTES + HTTP_STATUS_BYTES;
    child
        .stdout
        .take()
        .ok_or("provider")?
        .take((output_limit + 1) as u64)
        .read_to_end(&mut raw)
        .await
        .map_err(curl_io_failure)?;
    if raw.len() > output_limit {
        tracing::warn!("OpenAI assessment response exceeded the size limit");
        return Err("provider");
    }
    let exit = child.wait().await.map_err(curl_io_failure)?;
    let (body, status) = split_http_status(&raw).unwrap_or((&[], 0));
    if !exit.success() || !(200..300).contains(&status) {
        tracing::warn!(
            http_status = status,
            curl_exit = ?exit.code(),
            provider_code = provider_error_code(body),
            "OpenAI assessment request failed"
        );
        return Err("provider");
    }
    parse(body)
}

fn curl_io_failure(error: std::io::Error) -> &'static str {
    tracing::warn!(kind = ?error.kind(), "OpenAI assessment transport I/O failed");
    "provider"
}

fn split_http_status(raw: &[u8]) -> Option<(&[u8], u16)> {
    let (body, [separator, digits @ ..]) = raw.split_last_chunk::<HTTP_STATUS_BYTES>()?;
    if *separator != b'\n' || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some((body, std::str::from_utf8(digits).ok()?.parse().ok()?))
}

fn provider_error_code(body: &[u8]) -> &'static str {
    let Ok(error) = serde_json::from_slice::<Value>(body) else {
        return "unknown";
    };
    match error["error"]["code"].as_str() {
        Some("insufficient_quota") => "insufficient_quota",
        Some("invalid_api_key") => "invalid_api_key",
        Some("model_not_found") => "model_not_found",
        Some("rate_limit_exceeded") => "rate_limit_exceeded",
        Some("context_length_exceeded") => "context_length_exceeded",
        _ => "unknown",
    }
}

#[derive(Deserialize)]
struct Response {
    status: String,
    output: Vec<Output>,
}

#[derive(Deserialize)]
struct Output {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    content: Vec<Content>,
}

#[derive(Deserialize)]
struct Content {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

fn parse(raw: &[u8]) -> Result<Assessment, &'static str> {
    let parsed: Response = serde_json::from_slice(raw).map_err(|_| {
        tracing::warn!("OpenAI assessment returned an invalid Responses envelope");
        "provider"
    })?;
    match parsed.status.as_str() {
        "completed" => {}
        "incomplete" => {
            tracing::warn!("OpenAI assessment returned an incomplete response");
            return Err("provider");
        }
        _ => {
            tracing::warn!("OpenAI assessment response did not complete");
            return Err("provider");
        }
    }
    let mut contents = parsed
        .output
        .into_iter()
        .filter(|item| item.kind == "message")
        .flat_map(|item| item.content);
    let output = contents.next().ok_or("invalid_report")?;
    if output.kind != "output_text" || contents.next().is_some() {
        return Err("invalid_report");
    }
    serde_json::from_str(output.text.as_deref().ok_or("invalid_report")?)
        .map_err(|_| "invalid_report")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separates_curl_status_from_the_response_body() {
        let body = br#"{"status":"completed","output":[]}"#;
        let mut output = body.to_vec();
        output.extend_from_slice(b"\n200");
        assert_eq!(split_http_status(&output), Some((body.as_slice(), 200)));
        assert_eq!(split_http_status(b"\n000"), Some((b"".as_slice(), 0)));
        assert_eq!(split_http_status(body), None);
    }

    #[test]
    fn provider_error_codes_omit_untrusted_text() {
        assert_eq!(
            provider_error_code(br#"{"error":{"code":"insufficient_quota","message":"private"}}"#),
            "insufficient_quota"
        );
        assert_eq!(
            provider_error_code(br#"{"error":{"code":"arbitrary upstream text"}}"#),
            "unknown"
        );
    }

    #[test]
    fn requests_a_strict_schema_with_all_ten_topics_and_ignores_reasoning_items() {
        let request = request(DEFAULT_MODEL, &json!({"target_player":"white"}));
        let topic = &request["text"]["format"]["schema"]["properties"]["topics"]["items"]["properties"]
            ["topic"];
        assert_eq!(topic["type"], "string");
        assert_eq!(topic["enum"].as_array().unwrap().len(), 10);
        let report = json!({"summary":"Summary","next_match_focus":"Focus",
            "topics":[],"key_decisions":[],"training":[],"limitations":[]});
        let response = json!({"status":"completed","output":[{"type":"reasoning"},
            {"type":"message","content":[{"type":"output_text","text":report.to_string()}]}]});
        assert_eq!(
            parse(&serde_json::to_vec(&response).unwrap())
                .unwrap()
                .summary,
            "Summary"
        );
        assert!(parse(br#"{"status":"incomplete","output":[]}"#).is_err());
        assert!(parse(br#"{"status":"completed","output":[{"type":"message","content":[{"type":"refusal"}]}]}"#).is_err());
    }
}
