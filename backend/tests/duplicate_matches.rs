use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
};
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

fn upload(database: Uuid, filename: &str, bytes: &[u8]) -> Request<Body> {
    let mut body=format!("--bg\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n--bg--\r\n");
    Request::builder()
        .method("POST")
        .uri(format!("/api/imports?database_id={database}"))
        .header("content-type", "multipart/form-data; boundary=bg")
        .body(Body::from(body))
        .unwrap()
}

async fn import(app: &axum::Router, database: Uuid, filename: &str, bytes: &[u8]) -> String {
    let response = app
        .clone()
        .oneshot(upload(database, filename, bytes))
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status, 201, "{value}");
    value["match"]["id"].as_str().unwrap().to_owned()
}

async fn get(app: &axum::Router, path: &str) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status().as_u16();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

fn member_ids(group: &Value) -> Vec<&str> {
    group["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect()
}

async fn database(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO databases(id,name) VALUES(gen_random_uuid(),$1) RETURNING id")
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicates_group_the_same_played_record_across_formats_and_databases(pool: PgPool) {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new("/unavailable".into()),
    ));
    let a = database(&pool, "Duplicates A").await;
    let b = database(&pool, "Duplicates B").await;
    let xg = include_bytes!("fixtures/WC2014.xg").as_slice();
    let sgf = include_bytes!("fixtures/WC2014.gnubg.sgf").as_slice();
    let bgf = include_bytes!("fixtures/tutorial.bgf").as_slice();
    let xg_a = import(&app, a, "WC2014.xg", xg).await;
    let sgf_a = import(&app, a, "WC2014.sgf", sgf).await;
    let bgf_a = import(&app, a, "tutorial.bgf", bgf).await;
    let xg_b = import(&app, b, "WC2014 copy.xg", xg).await;
    let bgf_b = import(&app, b, "tutorial.bgf", bgf).await;

    // The same match from XG and GNUbg SGF sources has the same played record.
    let (status, page) = get(&app, "/api/library/duplicates").await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["total_groups"], 2, "{page}");
    assert_eq!(page["total_matches"], 5);
    assert!(page["next_cursor"].is_null());
    let groups = page["groups"].as_array().unwrap();
    assert_eq!(groups[0]["kind"], "same_moves");
    assert_eq!(groups[0]["match_count"], 3);
    assert_eq!(member_ids(&groups[0]), [&xg_a, &sgf_a, &xg_b]);
    assert_eq!(groups[0]["items"][0]["database_id"], a.to_string());
    assert_eq!(groups[1]["kind"], "identical_files");
    assert_eq!(member_ids(&groups[1]), [&bgf_a, &bgf_b]);

    // A database scope compares only that database's matches.
    let (_, scoped) = get(&app, &format!("/api/library/duplicates?database_id={a}")).await;
    assert_eq!(scoped["total_groups"], 1, "{scoped}");
    assert_eq!(scoped["groups"][0]["kind"], "same_moves");
    assert_eq!(member_ids(&scoped["groups"][0]), [&xg_a, &sgf_a]);
    let (_, scoped) = get(&app, &format!("/api/library/duplicates?database_id={b}")).await;
    assert_eq!(scoped["total_groups"], 0, "{scoped}");
    assert_eq!(scoped["groups"].as_array().unwrap().len(), 0);

    // Keyset pagination returns every group once with stable totals.
    let (_, first) = get(&app, "/api/library/duplicates?limit=1").await;
    assert_eq!(first["groups"].as_array().unwrap().len(), 1);
    assert_eq!(first["total_groups"], 2);
    let cursor = first["next_cursor"].as_str().unwrap();
    let (_, second) = get(
        &app,
        &format!("/api/library/duplicates?limit=1&after={cursor}"),
    )
    .await;
    assert_eq!(member_ids(&second["groups"][0]), [&bgf_a, &bgf_b]);
    assert_eq!(second["total_groups"], 2);
    assert!(second["next_cursor"].is_null());

    // A deleted copy leaves a group of two; a single remaining match is no duplicate.
    for id in [&xg_b, &bgf_b] {
        sqlx::query("DELETE FROM matches WHERE id=$1::uuid")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let (_, page) = get(&app, "/api/library/duplicates").await;
    assert_eq!(page["total_groups"], 1, "{page}");
    assert_eq!(member_ids(&page["groups"][0]), [&xg_a, &sgf_a]);

    for path in [
        "/api/library/duplicates?limit=0",
        "/api/library/duplicates?limit=101",
        "/api/library/duplicates?after=abc",
        "/api/library/duplicates?after=-1",
        "/api/library/duplicates?database_id=not-a-uuid",
    ] {
        assert_eq!(get(&app, path).await.0, 422, "{path}");
    }
    let missing = Uuid::new_v4();
    assert_eq!(
        get(
            &app,
            &format!("/api/library/duplicates?database_id={missing}")
        )
        .await
        .0,
        404
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicates_ignore_paused_matches_and_saved_position_context(pool: PgPool) {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new("/unavailable".into()),
    ));
    let a = database(&pool, "Duplicates A").await;
    let bgf = include_bytes!("fixtures/tutorial.bgf").as_slice();
    import(&app, a, "tutorial.bgf", bgf).await;
    let other = database(&pool, "Duplicates B").await;
    let copy = import(&app, other, "tutorial.bgf", bgf).await;
    let (_, page) = get(&app, "/api/library/duplicates").await;
    assert_eq!(page["total_groups"], 1, "{page}");
    // Paused play checkpoints are resumable sessions, not library duplicates.
    sqlx::query(
        "UPDATE matches SET database_id='00000000-0000-0000-0000-000000000004' WHERE id=$1::uuid",
    )
    .bind(&copy)
    .execute(&pool)
    .await
    .unwrap();
    let (_, page) = get(&app, "/api/library/duplicates").await;
    assert_eq!(page["total_groups"], 0, "{page}");
    let (status, _) = get(
        &app,
        "/api/library/duplicates?database_id=00000000-0000-0000-0000-000000000002",
    )
    .await;
    assert_eq!(status, 404, "saved-position context is no library database");
}

#[sqlx::test(migrations = "./migrations")]
async fn one_different_position_or_match_length_is_no_duplicate(pool: PgPool) {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new("/unavailable".into()),
    ));
    let a = database(&pool, "Near duplicates").await;
    // Same length and position count; the third match differs in one board,
    // the fourth only in match length. The second records the dice reversed.
    for (n, length, positions) in [
        (1u128, 7, ["4HPwATDgc/ABMA", "4HPkQSDgc/ABMA"]),
        (2, 7, ["4HPwATDgc/ABMA", "4HPkQSDgc/ABMA"]),
        (3, 7, ["4HPwATDgc/ABMA", "4OvgATDgc+QBUA"]),
        (4, 5, ["4HPwATDgc/ABMA", "4HPkQSDgc/ABMA"]),
    ] {
        let id = Uuid::from_u128(n);
        sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,database_id) VALUES($1,'near.sgf','sgf',$2,'A','B',$3,2,''::bytea,'',$4)")
            .bind(id).bind(id.to_string()).bind(length).bind(a).execute(&pool).await.unwrap();
        for (ordinal, position_id) in positions.iter().enumerate() {
            let dice = if n == 2 { [2, 6] } else { [6, 2] };
            sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) VALUES(gen_random_uuid(),$1,$2,1,$3,0,$4,array_fill(0,ARRAY[26]),$5,'MAFrAQAAAAAE','24/18 13/11')")
                .bind(id).bind(ordinal as i32).bind(ordinal as i32 + 1).bind(dice).bind(position_id).execute(&pool).await.unwrap();
        }
    }
    let (_, page) = get(&app, "/api/library/duplicates").await;
    assert_eq!(page["total_groups"], 1, "{page}");
    assert_eq!(page["groups"][0]["kind"], "same_moves");
    let one = Uuid::from_u128(1).to_string();
    let two = Uuid::from_u128(2).to_string();
    assert_eq!(member_ids(&page["groups"][0]), [&one, &two]);
}

async fn stored_and_computed(pool: &PgPool, id: &str) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    sqlx::query_as(
        "SELECT played_record_fingerprint,played_record_fingerprint(id) FROM matches WHERE id=$1::uuid",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn stored_fingerprint_follows_every_write_to_positions(pool: PgPool) {
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new("/unavailable".into()),
    ));
    let a = database(&pool, "Fingerprint A").await;
    let b = database(&pool, "Fingerprint B").await;
    let source = import(&app, a, "WC2014.xg", include_bytes!("fixtures/WC2014.xg")).await;
    let (stored, computed) = stored_and_computed(&pool, &source).await;
    assert!(stored.is_some(), "an import stores the played record");
    assert_eq!(stored, computed);
    let original = stored;

    // A copy receives its positions in a separate statement and the same record.
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/library/actions")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "action": "copy",
                        "match_ids": [source],
                        "database_id": a,
                        "target_database_id": b,
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let result: Value = serde_json::from_slice(&body).unwrap();
    let copy = result["results"][0]["created_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(stored_and_computed(&pool, &copy).await.0, original);

    // An appended position, as an archived play session writes it, changes the record.
    sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) SELECT gen_random_uuid(),match_uuid,ordinal+1,game_no,ply+1,player,dice,board,position_id,gnubg_match_id,notation FROM positions WHERE match_uuid=$1::uuid ORDER BY ordinal DESC LIMIT 1")
        .bind(&copy)
        .execute(&pool)
        .await
        .unwrap();
    let (appended, computed) = stored_and_computed(&pool, &copy).await;
    assert_eq!(appended, computed);
    assert_ne!(appended, original);

    // Moving a position refreshes both matches; removing all positions clears it.
    sqlx::query("UPDATE positions SET match_uuid=$2::uuid WHERE id=(SELECT id FROM positions WHERE match_uuid=$1::uuid ORDER BY ordinal DESC LIMIT 1)")
        .bind(&copy)
        .bind(&source)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(stored_and_computed(&pool, &copy).await.0, original);
    let (moved, computed) = stored_and_computed(&pool, &source).await;
    assert_eq!(moved, computed);
    assert_ne!(moved, original);
    sqlx::query("DELETE FROM positions WHERE match_uuid=$1::uuid")
        .bind(&copy)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(stored_and_computed(&pool, &copy).await, (None, None));
}
