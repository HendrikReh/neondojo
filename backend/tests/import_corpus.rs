//! Optional local corpus: every upload runs in a disposable SQLx database.
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
    model::ImportResponse,
};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tower::ServiceExt;

fn files(dir: &Path, extension: &str, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, extension, output);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case(extension))
        {
            output.push(path);
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL, GNUbg and the optional local games/{xg,mat,bgf} corpus"]
async fn all_local_game_files_import_atomically_and_deduplicate(pool: PgPool) {
    let root = std::env::var_os("NEONDOJO_GAMES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("games")
        });
    if !["xg", "mat", "bgf"]
        .iter()
        .all(|kind| root.join(kind).is_dir())
    {
        eprintln!(
            "SKIP local corpus: games/xg, games/mat and games/bgf are not present at {}",
            root.display()
        );
        return;
    }
    let state = AppState::new(
        pool.clone(),
        Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into())),
    );
    let app = api::router(state);
    let mut paths = Vec::new();
    for kind in ["xg", "mat", "bgf"] {
        files(&root.join(kind), kind, &mut paths);
    }
    paths.sort();
    assert!(!paths.is_empty());
    let mut seen = HashMap::new();
    let mut failures = Vec::new();
    let mut report = Vec::new();
    for path in &paths {
        let bytes = std::fs::read(path).unwrap();
        let digest = format!("{:x}", Sha256::digest(&bytes));
        let filename = path.file_name().unwrap().to_str().unwrap();
        let mut body=format!("--bgcorpus\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
        body.extend_from_slice(&bytes);
        body.extend_from_slice(b"\r\n--bgcorpus--\r\n");
        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM matches")
            .fetch_one(&pool)
            .await
            .unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/imports")
                    .header("content-type", "multipart/form-data; boundary=bgcorpus")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status().as_u16();
        let data = response.into_body().collect().await.unwrap().to_bytes();
        let result: serde_json::Value = serde_json::from_slice(&data).unwrap();
        let after: i64 = sqlx::query_scalar("SELECT count(*) FROM matches")
            .fetch_one(&pool)
            .await
            .unwrap();
        let relative = path.strip_prefix(&root).unwrap().display().to_string();
        if status == 200 || status == 201 {
            let imported: ImportResponse = serde_json::from_value(result.clone()).unwrap();
            if let Some(previous) = seen.get(&digest) {
                assert_eq!(status, 200, "{relative}");
                assert!(imported.duplicate, "{relative}");
                assert_eq!(&imported.r#match.id, previous, "{relative}");
                assert_eq!(after, before);
            } else {
                assert_eq!(status, 201, "{relative}");
                assert!(!imported.duplicate);
                assert_eq!(after, before + 1);
                seen.insert(digest, imported.r#match.id);
            }
            let persisted: i64 =
                sqlx::query_scalar("SELECT count(*) FROM positions WHERE match_uuid=$1")
                    .bind(imported.r#match.id)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert!(persisted > 0);
            assert_eq!(persisted, imported.r#match.position_count as i64);
            println!("PASS {relative}: HTTP {status}, {persisted} positions");
        } else {
            assert_eq!(
                after, before,
                "failed upload left a partial match: {relative}"
            );
            let failure = format!("{relative}: HTTP {status}: {result}");
            eprintln!("FAIL {failure}");
            failures.push(failure);
        }
        report.push(serde_json::json!({"file":relative,"status":status,"response":result}));
    }
    if let Some(path) = std::env::var_os("NEONDOJO_CORPUS_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    }
    println!(
        "{} files checked, {} unique imports, {} failures",
        paths.len(),
        seen.len(),
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
