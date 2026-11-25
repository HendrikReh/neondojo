//! The API only answers browsers on a loopback host and refuses state changes
//! from foreign origins (DNS rebinding and cross-site request forgery).
use crate::support;
use axum::{Router, body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const BOUNDARY: &str = "neondojo-boundary";

fn multipart(field: &str) -> Body {
    Body::from(format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"m.sgf\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n{}\r\n--{BOUNDARY}--\r\n",
        support::FOREIGN_SOURCE
    ))
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, &str)],
    body: Body,
) -> (u16, Value) {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn benchmark_runs(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM benchmark_runs")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn matches(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM matches")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn frontend_and_api_share_the_local_access_guard(pool: PgPool) {
    use neondojo::{api, engine::Engine};

    let dir = tempfile::tempdir().unwrap();
    let index = "<html>NeonDojo</html>";
    std::fs::write(dir.path().join("index.html"), index).unwrap();
    std::fs::write(dir.path().join("app.js"), "boot()").unwrap();
    let state = api::AppState::new(pool, Engine::new("/missing/gnubg".into()));
    let app = api::router_with_web_dir(state, Some(dir.path()));

    for (uri, expected) in [("/", index), ("/app.js", "boot()")] {
        let response = app
            .clone()
            .oneshot(
                Request::get(uri)
                    .header("host", "localhost")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .as_ref(),
            expected.as_bytes()
        );
    }
    for uri in ["/", "/app.js", "/client-route"] {
        let (status, body) = send(
            &app,
            "GET",
            uri,
            &[("host", "outside.example")],
            Body::empty(),
        )
        .await;
        assert_eq!(status, 403);
        assert!(body["error"].as_str().unwrap().contains("loopback"));
    }
    let (status, body) = send(
        &app,
        "GET",
        "/api/unknown-route",
        &[("host", "localhost")],
        Body::empty(),
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["error"], "Not found");
}

#[sqlx::test(migrations = "./migrations")]
async fn foreign_hosts_are_refused_on_every_route(pool: PgPool) {
    let app = support::app(&pool, "/missing/gnubg");
    let multipart_type = format!("multipart/form-data; boundary={BOUNDARY}");
    for host in ["rebind.example", "rebind.example:8787", "192.168.1.20:8787"] {
        for (method, uri, content_type, body) in [
            ("GET", "/api/health", "application/json", Body::empty()),
            // Merged outside the restore-drain middleware; must be covered too.
            (
                "GET",
                "/api/database-backup",
                "application/json",
                Body::empty(),
            ),
            (
                "POST",
                "/api/database-backup/check",
                "application/octet-stream",
                Body::empty(),
            ),
            (
                "GET",
                "/api/unknown-route",
                "application/json",
                Body::empty(),
            ),
            ("OPTIONS", "/api/imports", "application/json", Body::empty()),
            ("POST", "/api/imports", &multipart_type, multipart("file")),
        ] {
            let (status, body) = send(
                &app,
                method,
                uri,
                &[("host", host), ("content-type", content_type)],
                body,
            )
            .await;
            assert_eq!(status, 403, "{method} {uri} via {host}: {body}");
            assert!(
                body["error"].as_str().unwrap().contains("loopback"),
                "{body}"
            );
        }
    }
    assert_eq!(matches(&pool).await, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn cross_site_state_changes_are_refused(pool: PgPool) {
    let app = support::app(&pool, "/missing/gnubg");
    let multipart_type = format!("multipart/form-data; boundary={BOUNDARY}");
    let job = format!("/api/jobs/{}/cancel", Uuid::new_v4());
    let benchmark = format!("/api/benchmarks/{}/cancel", Uuid::new_v4());
    let refused: [&[(&str, &str)]; 4] = [
        &[("origin", "https://attacker.example")],
        // Sandboxed frames (such as Anki card previews) and file: pages.
        &[("origin", "null")],
        &[("origin", "http://localhost.attacker.example:8787")],
        &[("sec-fetch-site", "cross-site")],
    ];
    for headers in refused {
        let with = |content_type| {
            let mut all = vec![("host", "127.0.0.1:8787"), ("content-type", content_type)];
            all.extend_from_slice(headers);
            all
        };
        for (uri, content_type, body) in [
            ("/api/imports", multipart_type.as_str(), multipart("file")),
            ("/api/benchmarks/play", "text/plain", Body::empty()),
            ("/api/benchmarks/play", "application/json", Body::empty()),
            (job.as_str(), "text/plain", Body::empty()),
            (benchmark.as_str(), "text/plain", Body::empty()),
        ] {
            let (status, body) = send(&app, "POST", uri, &with(content_type), body).await;
            assert_eq!(status, 403, "{uri} with {headers:?}: {body}");
            assert!(body["error"].is_string(), "{body}");
        }
        let (status, body) = send(
            &app,
            "DELETE",
            &format!("/api/databases/{}", Uuid::new_v4()),
            &with("application/json"),
            Body::empty(),
        )
        .await;
        assert_eq!(status, 403, "DELETE with {headers:?}: {body}");
    }
    assert_eq!(matches(&pool).await, 0);
    assert_eq!(benchmark_runs(&pool).await, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn local_and_headerless_clients_are_served(pool: PgPool) {
    let app = support::app(&pool, "/missing/gnubg");
    let multipart_type = format!("multipart/form-data; boundary={BOUNDARY}");
    let cancel = format!("/api/jobs/{}/cancel", Uuid::new_v4());
    let allowed: [&[(&str, &str)]; 6] = [
        // Playwright and Vite proxy from a loopback listener on any port.
        &[
            ("host", "127.0.0.1:61234"),
            ("origin", "http://127.0.0.1:61234"),
            ("sec-fetch-site", "same-origin"),
        ],
        &[
            ("host", "localhost:5173"),
            ("origin", "http://localhost:5173"),
        ],
        &[("host", "[::1]:8787"), ("origin", "http://[::1]:8787")],
        // Tauri's WebView calls the API from its custom scheme.
        &[
            ("host", "127.0.0.1:8787"),
            ("origin", "tauri://localhost"),
            ("sec-fetch-site", "cross-site"),
        ],
        &[("host", "localhost:8787")],
        // Command-line clients and in-process callers send neither header.
        &[],
    ];
    for headers in allowed {
        // Health is "degraded" (503) here because the test has no GNUbg.
        let (_, body) = send(&app, "GET", "/api/health", headers, Body::empty()).await;
        assert_eq!(body["database"], true, "{headers:?}: {body}");

        let (status, body) = send(&app, "POST", &cancel, headers, Body::empty()).await;
        assert_eq!(status, 404, "{headers:?}: {body}");

        let mut with_type = headers.to_vec();
        with_type.push(("content-type", &multipart_type));
        let (status, body) = send(
            &app,
            "POST",
            "/api/imports",
            &with_type,
            multipart("not-file"),
        )
        .await;
        assert_eq!(status, 422, "{headers:?}: {body}");
        assert_eq!(body["error"], "Expected multipart field 'file'");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn play_benchmarks_require_a_json_body_type(pool: PgPool) {
    let app = support::app(&pool, "/missing/gnubg");
    // Without a JSON Content-Type, a browser could send this without a CORS
    // preflight; it is refused before any settings or engine work.
    for headers in [
        &[("content-type", "text/plain")][..],
        &[("content-type", "application/x-www-form-urlencoded")][..],
        &[][..],
    ] {
        for body in ["", "{\"engine_id\":\"gnubg\",\"plies\":1}"] {
            let (status, response) = send(
                &app,
                "POST",
                "/api/benchmarks/play",
                headers,
                Body::from(body),
            )
            .await;
            assert_eq!(status, 422, "{headers:?} {body:?}: {response}");
            assert!(
                response["error"]
                    .as_str()
                    .unwrap()
                    .contains("Content-Type: application/json"),
                "{response}"
            );
        }
    }
    // An empty body or `null` still selects the saved play profile, which then
    // fails only because this test has no engine.
    for body in ["", "null"] {
        let (status, response) = send(
            &app,
            "POST",
            "/api/benchmarks/play",
            &[("content-type", "application/json")],
            Body::from(body),
        )
        .await;
        assert_eq!(status, 422, "{body:?}: {response}");
        assert!(
            response["error"]
                .as_str()
                .unwrap()
                .starts_with("Engine unavailable"),
            "{body:?}: {response}"
        );
    }
    let (status, response) = send(
        &app,
        "POST",
        "/api/benchmarks/play",
        &[("content-type", "application/json; charset=utf-8")],
        Body::from("{\"engine_id\":\"gnubg\",\"plies\":99}"),
    )
    .await;
    assert_eq!(status, 422, "{response}");
    assert_eq!(response["error"], "Unsupported play benchmark profile");
    let (status, response) = send(
        &app,
        "POST",
        "/api/benchmarks/play",
        &[("content-type", "application/json")],
        Body::from("{"),
    )
    .await;
    assert_eq!(status, 422, "{response}");
    assert_eq!(benchmark_runs(&pool).await, 0);
}
