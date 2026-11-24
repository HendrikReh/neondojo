//! Every documented endpoint answers a malformed request with the JSON
//! `ErrorBody`. The requests come from the OpenAPI document, so a new endpoint
//! is covered without a change here.
use crate::support;
use axum::{
    Router,
    body::Body,
    http::{Request, header},
};
use http_body_util::BodyExt;
use neondojo::api;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use utoipa::OpenApi;
use uuid::Uuid;

/// These answer a malformed id with the 422 they document instead of 400.
const INVALID_ID: [&str; 2] = ["/api/positions/{id}/bookmark", "/api/decks/{id}"];

struct Operation {
    method: String,
    path: String,
    spec: Value,
}

fn operations() -> Vec<Operation> {
    let document = serde_json::to_value(api::ApiDoc::openapi()).unwrap();
    let mut operations = Vec::new();
    for (path, methods) in document["paths"].as_object().unwrap() {
        for (method, spec) in methods.as_object().unwrap() {
            operations.push(Operation {
                method: method.to_uppercase(),
                path: path.clone(),
                spec: spec.clone(),
            });
        }
    }
    operations
}

impl Operation {
    fn parameters(&self, location: &str) -> Vec<&Value> {
        self.spec["parameters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|parameter| parameter["in"] == location)
            .collect()
    }

    fn body_type(&self) -> Option<&str> {
        Some(
            self.spec["requestBody"]["content"]
                .as_object()?
                .keys()
                .next()?,
        )
    }

    /// The path with each parameter replaced.
    fn uri(&self, value: impl Fn(&Value) -> String) -> String {
        self.parameters("path")
            .into_iter()
            .fold(self.path.clone(), |uri, parameter| {
                let name = parameter["name"].as_str().unwrap();
                uri.replace(&format!("{{{name}}}"), &value(parameter))
            })
    }

    /// The path with well-formed parameters that match no record.
    fn unknown_uri(&self) -> String {
        self.uri(|parameter| {
            let schema = &parameter["schema"];
            match schema["type"].as_str() {
                Some("integer") => "1".into(),
                _ if schema["format"] == "uuid" => Uuid::new_v4().to_string(),
                _ => "unknown".into(),
            }
        })
    }
}

fn is_uuid(parameter: &Value) -> bool {
    parameter["schema"]["format"] == "uuid"
}

/// Sends a malformed request and returns the status after checking the body.
async fn rejected(app: &Router, method: &str, uri: &str, content_type: &str, body: &str) -> u16 {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body.to_owned()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let content_type = response.headers().get(header::CONTENT_TYPE).cloned();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes);
    assert_eq!(
        content_type.as_ref().and_then(|value| value.to_str().ok()),
        Some("application/json"),
        "{method} {uri} {status}: {text}"
    );
    let error: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        error["error"].as_str().is_some_and(|text| !text.is_empty()),
        "{method} {uri} {status}: {error}"
    );
    status
}

#[sqlx::test(migrations = "./migrations")]
async fn malformed_ids_return_the_json_error_body(pool: PgPool) {
    let app = support::app(&pool, "/unavailable");
    let mut checked = 0;
    for operation in operations() {
        if !operation.parameters("path").into_iter().any(is_uuid) {
            continue;
        }
        let uri = operation.uri(|parameter| {
            if is_uuid(parameter) {
                "not-a-uuid".into()
            } else {
                "1".into()
            }
        });
        // The deck routes read other parts of the request before the id.
        let (uri, body) = match (operation.method.as_str(), operation.path.as_str()) {
            ("DELETE", "/api/decks/{id}") => (format!("{uri}?expected_version=0"), "{}"),
            ("PUT", "/api/decks/{id}") => (uri, r#"{"name":"Deck","expected_version":0}"#),
            _ => (uri, "{}"),
        };
        let status = rejected(&app, &operation.method, &uri, "application/json", body).await;
        let expected = if INVALID_ID.contains(&operation.path.as_str()) {
            422
        } else {
            400
        };
        assert_eq!(status, expected, "{} {uri}", operation.method);
        checked += 1;
    }
    assert!(
        checked > 90,
        "only {checked} operations have a UUID in the path"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn malformed_bodies_return_the_json_error_body(pool: PgPool) {
    let app = support::app(&pool, "/unavailable");
    let (mut json, mut multipart) = (0, 0);
    for operation in operations() {
        let uri = operation.unknown_uri();
        match operation.body_type() {
            Some("application/json") => {
                // 404 where a missing record is reported before the body, and
                // 422 otherwise.
                let status = rejected(&app, &operation.method, &uri, "application/json", "{").await;
                assert!(
                    matches!(status, 404 | 422),
                    "{} {uri}: {status}",
                    operation.method
                );
                json += 1;
            }
            Some("multipart/form-data") => {
                let status =
                    rejected(&app, &operation.method, &uri, "application/json", "{}").await;
                assert_eq!(status, 400, "{} {uri}", operation.method);
                multipart += 1;
            }
            _ => {}
        }
    }
    assert!(json > 60, "only {json} operations take a JSON body");
    assert_eq!(multipart, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn malformed_queries_return_the_json_error_body(pool: PgPool) {
    let app = support::app(&pool, "/unavailable");
    let mut checked = 0;
    for operation in operations() {
        let Some(parameter) = operation.parameters("query").into_iter().find(|parameter| {
            let schema = &parameter["schema"];
            schema["type"] == "integer"
                || schema["type"]
                    .as_array()
                    .is_some_and(|types| types.contains(&"integer".into()))
        }) else {
            continue;
        };
        let uri = format!(
            "{}?{}=abc",
            operation.unknown_uri(),
            parameter["name"].as_str().unwrap()
        );
        let status = rejected(&app, &operation.method, &uri, "application/json", "{}").await;
        assert_eq!(status, 422, "{} {uri}", operation.method);
        checked += 1;
    }
    assert!(
        checked > 20,
        "only {checked} operations have an integer query"
    );
}

/// A parameter hidden in a `Result<Query<T>, _>` argument loses its location.
#[test]
fn parameters_are_documented_where_they_are_sent() {
    for operation in operations() {
        for (location, in_path) in [("path", true), ("query", false)] {
            for parameter in operation.parameters(location) {
                let name = parameter["name"].as_str().unwrap();
                assert_eq!(
                    operation.path.contains(&format!("{{{name}}}")),
                    in_path,
                    "{} {} documents `{name}` as a {location} parameter",
                    operation.method,
                    operation.path
                );
            }
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn unknown_paths_and_methods_return_the_json_error_body(pool: PgPool) {
    // The backend serves the web client as the fallback for every other path.
    let app = support::app(&pool, "/unavailable").fallback(|| async { "web client" });
    for method in ["GET", "POST", "PUT", "PATCH", "DELETE"] {
        for uri in [
            "/api",
            "/api/",
            "/api/unknown",
            "/api/health/unknown",
            "/api/matches/",
            "/api/matches/1/unknown/2",
        ] {
            let status = rejected(&app, method, uri, "application/json", "{}").await;
            assert_eq!(status, 404, "{method} {uri}");
        }
    }
    let page = app
        .clone()
        .oneshot(Request::get("/apiary").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let page = page.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&page[..], b"web client");

    for operation in operations() {
        // No endpoint has this method.
        let uri = operation.unknown_uri();
        let status = rejected(&app, "TRACE", &uri, "application/json", "{}").await;
        assert_eq!(status, 405, "{uri}");
    }

    let response = app
        .oneshot(Request::post("/api/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()[header::ALLOW], "GET,HEAD");
}
