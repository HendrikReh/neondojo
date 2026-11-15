//! The browser never receives the API key or the upstream response body.
use super::{Assessment, TopicName};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use utoipa::PartialSchema;

const DEFAULT_MODEL: &str = "gpt-6.1-terra";
const MAX_RESPONSE_BYTES: usize = 256 * 1024;

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
    std::env::var("BGTUTOR_ASSESSMENT_MODEL")
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
            "type":"json_schema","name":"bg_tutor_match_assessment","strict":true,
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
        .map_err(|_| "provider")?
}

async fn send(config: String) -> Result<Assessment, &'static str> {
    let mut child = tokio::process::Command::new("/usr/bin/curl")
        .args([
            "-q",
            "--silent",
            "--fail",
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
            "https://api.openai.com/v1/responses",
        ])
        .env_remove("OPENAI_API_KEY")
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "provider")?;
    let mut stdin = child.stdin.take().ok_or("provider")?;
    stdin
        .write_all(config.as_bytes())
        .await
        .map_err(|_| "provider")?;
    drop(stdin);
    let mut raw = Vec::new();
    child
        .stdout
        .take()
        .ok_or("provider")?
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut raw)
        .await
        .map_err(|_| "provider")?;
    if raw.len() > MAX_RESPONSE_BYTES || !child.wait().await.map_err(|_| "provider")?.success() {
        return Err("provider");
    }
    parse(&raw)
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
    let parsed: Response = serde_json::from_slice(raw).map_err(|_| "provider")?;
    if parsed.status != "completed" {
        return Err("provider");
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
