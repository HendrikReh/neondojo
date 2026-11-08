use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    store,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn targets(pool: &PgPool, label: &str) -> (Uuid, Uuid) {
    let match_id = Uuid::new_v4();
    let position_id = Uuid::new_v4();
    sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf) VALUES($1,$2,'sgf',$3,'Alice','Bob',7,1,''::bytea,'private source')")
        .bind(match_id).bind(format!("{label}.sgf")).bind(match_id.to_string()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) VALUES($1,$2,0,2,7,0,ARRAY[3,1],array_fill(0,ARRAY[26]),'4HPwATDgc/ABMA','MIEEAAAAAAAE','8/5 6/5')")
        .bind(position_id).bind(match_id).execute(pool).await.unwrap();
    (match_id, position_id)
}

fn app(pool: &PgPool) -> axum::Router {
    api::router(AppState::new(pool.clone(), Engine::new("unused".into())))
}

async fn request(app: axum::Router, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let mut req = Request::builder().method(method).uri(path);
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let response = app
        .oneshot(
            req.body(
                body.map(|v| Body::from(v.to_string()))
                    .unwrap_or_else(Body::empty),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)}));
    (status, body)
}

async fn save(
    app: axum::Router,
    kind: &str,
    id: Uuid,
    tags: Vec<&str>,
    comment: &str,
    version: i64,
) -> (u16, Value) {
    request(
        app,
        "PUT",
        &format!("/api/{kind}/{id}/annotation"),
        Some(json!({"tags":tags,"comment":comment,"expected_version":version})),
    )
    .await
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn empty_annotations_are_distinct_from_missing_targets(pool: PgPool) {
    let (match_id, position_id) = targets(&pool, "empty").await;
    for (kind, id, target_kind) in [
        ("matches", match_id, "match"),
        ("positions", position_id, "position"),
    ] {
        let (status, annotation) = request(
            app(&pool),
            "GET",
            &format!("/api/{kind}/{id}/annotation"),
            None,
        )
        .await;
        assert_eq!(status, 200, "{annotation}");
        assert_eq!(
            annotation,
            json!({"id":null,"target_kind":target_kind,"target_id":id,"tags":[],"comment":"","version":0,"updated_at":null})
        );
        let (status, _) = request(
            app(&pool),
            "GET",
            &format!("/api/{kind}/{}/annotation", Uuid::new_v4()),
            None,
        )
        .await;
        assert_eq!(status, 404);
        assert_eq!(
            save(app(&pool), kind, Uuid::new_v4(), vec![], "", 0)
                .await
                .0,
            404
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn annotations_normalize_tags_and_preserve_plaintext(pool: PgPool) {
    let (match_id, position_id) = targets(&pool, "normalization").await;
    let comment = "## Cube decision\nKeep <script> as text\t& study.";
    for (kind, id) in [("matches", match_id), ("positions", position_id)] {
        let (status, created) = save(
            app(&pool),
            kind,
            id,
            vec!["  Cube   ACTION  ", "cube action", "RACE", "   "],
            comment,
            0,
        )
        .await;
        assert_eq!(status, 200, "{created}");
        assert_eq!(created["tags"], json!(["cube action", "race"]));
        assert_eq!(created["comment"], comment);
        assert_eq!(created["version"], 1);
        assert!(created["id"].as_str().is_some());
        assert!(created["updated_at"].as_str().is_some());
        let (_, fetched) = request(
            app(&pool),
            "GET",
            &format!("/api/{kind}/{id}/annotation"),
            None,
        )
        .await;
        assert_eq!(fetched, created);
        let (status, updated) = save(app(&pool), kind, id, vec![], "", 1).await;
        assert_eq!(status, 200, "{updated}");
        assert_eq!(updated["version"], 2);
        assert_eq!(updated["id"], created["id"]);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn concurrent_first_saves_and_updates_have_one_winner(pool: PgPool) {
    let (match_id, _) = targets(&pool, "concurrency").await;
    for version in [0, 1] {
        let (a, b) = tokio::join!(
            save(
                app(&pool),
                "matches",
                match_id,
                vec!["one"],
                "first",
                version
            ),
            save(
                app(&pool),
                "matches",
                match_id,
                vec!["two"],
                "second",
                version
            ),
        );
        let mut statuses = [a.0, b.0];
        statuses.sort();
        assert_eq!(statuses, [200, 409], "{a:?} {b:?}");
        let (_, persisted) = request(
            app(&pool),
            "GET",
            &format!("/api/matches/{match_id}/annotation"),
            None,
        )
        .await;
        let winner = if a.0 == 200 { a.1 } else { b.1 };
        assert_eq!(persisted, winner);
        assert_eq!(persisted["version"], version + 1);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn annotation_validation_is_bounded_and_does_not_change_existing_data(pool: PgPool) {
    let (match_id, _) = targets(&pool, "validation").await;
    let path = format!("/api/matches/{match_id}/annotation");
    for body in [
        json!({"tags":["x".repeat(65)],"comment":"","expected_version":0}),
        json!({"tags":(0..31).map(|i| format!("tag{i}")).collect::<Vec<_>>(),"comment":"","expected_version":0}),
        json!({"tags":["bad\nlabel"],"comment":"","expected_version":0}),
        json!({"tags":[],"comment":"x".repeat(20_001),"expected_version":0}),
        json!({"tags":[],"comment":"bad\u{0000}note","expected_version":0}),
        json!({"tags":[],"comment":"","expected_version":-1}),
        json!({"tags":[],"comment":""}),
    ] {
        let (status, response) = request(app(&pool), "PUT", &path, Some(body)).await;
        assert_eq!(status, 422, "{response}");
    }
    let (_, untouched) = request(app(&pool), "GET", &path, None).await;
    assert_eq!(untouched["version"], 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn unicode_limits_and_learning_annotation_reads_use_characters(pool: PgPool) {
    let (match_id, _) = targets(&pool, "unicode").await;
    let empty = bgtutor::annotations::get_annotation(&pool, "match", match_id)
        .await
        .unwrap();
    assert_eq!(empty.version, 0);
    let tag = "Ä".repeat(64);
    let comment = "🧠".repeat(20_000);
    let (status, body) = save(app(&pool), "matches", match_id, vec![&tag], &comment, 0).await;
    assert_eq!(status, 200, "{body}");
    let saved = bgtutor::annotations::get_annotation(&pool, "match", match_id)
        .await
        .unwrap();
    assert_eq!(saved.tags, vec!["ä".repeat(64)]);
    assert_eq!(saved.comment, comment);
    assert_eq!(saved.version, 1);
    assert_eq!(
        save(
            app(&pool),
            "matches",
            match_id,
            vec![&"Ä".repeat(65)],
            "",
            1
        )
        .await
        .0,
        422
    );
    assert_eq!(
        save(
            app(&pool),
            "matches",
            match_id,
            vec![],
            &"🧠".repeat(20_001),
            1
        )
        .await
        .0,
        422
    );
    assert!(
        bgtutor::annotations::get_annotation(&pool, "match", Uuid::new_v4())
            .await
            .is_err()
    );
    assert!(
        bgtutor::annotations::get_annotation(&pool, "invalid", match_id)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn search_filters_paginates_and_returns_jump_metadata_without_source(pool: PgPool) {
    let (match_id, position_id) = targets(&pool, "search").await;
    assert_eq!(
        save(
            app(&pool),
            "matches",
            match_id,
            vec!["Cube"],
            "Rare needle %_mark",
            0
        )
        .await
        .0,
        200
    );
    assert_eq!(
        save(
            app(&pool),
            "positions",
            position_id,
            vec!["Cube", "Race"],
            "Rare needle position",
            0
        )
        .await
        .0,
        200
    );
    let (status, first) = request(
        app(&pool),
        "GET",
        "/api/annotations/search?q=needle&tag=CUBE&limit=1",
        None,
    )
    .await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    let cursor = first["next_cursor"].as_str().unwrap();
    let (status, second) = request(
        app(&pool),
        "GET",
        &format!("/api/annotations/search?q=needle&tag=cube&limit=1&after={cursor}"),
        None,
    )
    .await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert!(second["next_cursor"].is_null());
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);
    let (status, filtered) = request(
        app(&pool),
        "GET",
        "/api/annotations/search?tag=race&kind=position",
        None,
    )
    .await;
    assert_eq!(status, 200, "{filtered}");
    let hit = &filtered["items"][0];
    assert_eq!(hit["target_id"], position_id.to_string());
    assert_eq!(hit["match_uuid"], match_id.to_string());
    assert_eq!(hit["filename"], "search.sgf");
    assert_eq!(hit["white"], "Alice");
    assert_eq!(hit["black"], "Bob");
    assert_eq!(hit["game_no"], 2);
    assert_eq!(hit["ply"], 7);
    assert!(hit["comment_excerpt"].as_str().unwrap().contains("needle"));
    assert!(hit.get("sgf").is_none() && hit.get("original").is_none());
    let (_, literal) = request(
        app(&pool),
        "GET",
        "/api/annotations/search?q=%25_mark",
        None,
    )
    .await;
    assert_eq!(literal["items"].as_array().unwrap().len(), 1);
    assert_eq!(literal["items"][0]["target_kind"], "match");
    for query in [
        "",
        "q=ab",
        "q=ab&tag=cube",
        "q=abc%0A",
        "q=abc&limit=0",
        "q=abc&limit=101",
        "tag=cube&kind=invalid",
        "tag=cube&after=bad",
    ] {
        let (status, body) = request(
            app(&pool),
            "GET",
            &format!("/api/annotations/search?{query}"),
            None,
        )
        .await;
        assert_eq!(status, 422, "{query}: {body}");
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn tags_are_unique_sorted_normalized_and_searchable(pool: PgPool) {
    let (match_id, position_id) = targets(&pool, "tags").await;
    assert_eq!(
        save(
            app(&pool),
            "matches",
            match_id,
            vec!["Race", "Cube", "A%tag"],
            "",
            0
        )
        .await
        .0,
        200
    );
    assert_eq!(
        save(
            app(&pool),
            "positions",
            position_id,
            vec!["Race", "Racing"],
            "",
            0
        )
        .await
        .0,
        200
    );
    let (status, tags) = request(app(&pool), "GET", "/api/tags", None).await;
    assert_eq!(status, 200, "{tags}");
    assert_eq!(tags, json!(["a%tag", "cube", "race", "racing"]));
    let (_, tags) = request(app(&pool), "GET", "/api/tags?q=RA&limit=1", None).await;
    assert_eq!(tags, json!(["race"]));
    let (_, tags) = request(app(&pool), "GET", "/api/tags?q=%25", None).await;
    assert_eq!(tags, json!(["a%tag"]));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn library_search_includes_match_and_child_position_annotations(pool: PgPool) {
    let (match_id, position_id) = targets(&pool, "library").await;
    assert_eq!(
        save(app(&pool), "matches", match_id, vec![], "matchneedle", 0)
            .await
            .0,
        200
    );
    assert_eq!(
        save(
            app(&pool),
            "positions",
            position_id,
            vec!["positionneedle"],
            "",
            0
        )
        .await
        .0,
        200
    );
    for query in ["Alice", "matchneedle", "positionneedle"] {
        let matches = store::matches(&pool, query, None, 50).await.unwrap();
        assert_eq!(matches.items.len(), 1, "{query}");
        assert_eq!(matches.items[0].id, match_id);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn deleting_targets_cascades_annotations(pool: PgPool) {
    let (match_id, position_id) = targets(&pool, "cascade").await;
    assert_eq!(
        save(app(&pool), "matches", match_id, vec!["cube"], "a", 0)
            .await
            .0,
        200
    );
    assert_eq!(
        save(app(&pool), "positions", position_id, vec!["race"], "b", 0)
            .await
            .0,
        200
    );
    sqlx::query("DELETE FROM positions WHERE id=$1")
        .bind(position_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, tags) = request(app(&pool), "GET", "/api/tags", None).await;
    assert_eq!(tags, json!(["cube"]));
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, tags) = request(app(&pool), "GET", "/api/tags", None).await;
    assert_eq!(tags, json!([]));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "creates 10000 annotated matches to verify PostgreSQL search plans"]
async fn annotation_and_library_search_keep_indexed_hit_collection(pool: PgPool) {
    sqlx::raw_sql("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf)
        SELECT md5('annotation-match-'||n)::uuid,'indexed-'||n||'.sgf','sgf',md5(n::text),
        CASE WHEN n=4242 THEN 'Needle Player' ELSE 'Player '||n END,'Opponent',7,0,''::bytea,''
        FROM generate_series(1,10000) n;
        INSERT INTO annotations(id,match_uuid,tags,comment,version)
        SELECT md5('annotation-'||n)::uuid,md5('annotation-match-'||n)::uuid,
        CASE WHEN n=4242 THEN ARRAY['rare cube'] ELSE ARRAY['ordinary'] END,
        CASE WHEN n=4242 THEN 'needle annotation' ELSE repeat('ordinary comment ',20) END,1
        FROM generate_series(1,10000) n;
        ANALYZE matches; ANALYZE annotations;")
        .execute(&pool).await.unwrap();
    // Flush GIN's bulk-insert pending lists before measuring steady-state plans,
    // as in the existing large-library search benchmark.
    for sql in ["VACUUM ANALYZE matches", "VACUUM ANALYZE annotations"] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    let library_sql = "EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT id,filename,format,white,black,match_length,position_count,created_at FROM matches WHERE id IN (SELECT id FROM matches WHERE search_text LIKE '%needle%' UNION SELECT coalesce(a.match_uuid,p.match_uuid) FROM annotations a LEFT JOIN positions p ON p.id=a.position_uuid WHERE a.search_text LIKE '%needle%') ORDER BY id LIMIT 51";
    let library_plan: Value = sqlx::query_scalar(library_sql)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        library_plan.to_string().contains("matches_search_trgm"),
        "{library_plan:#}"
    );
    assert!(
        library_plan.to_string().contains("annotations_search_trgm"),
        "{library_plan:#}"
    );
    let tag_plan: Value = sqlx::query_scalar("EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) SELECT id FROM annotations WHERE tags @> ARRAY['rare cube'] ORDER BY id LIMIT 51")
        .fetch_one(&pool).await.unwrap();
    assert!(
        tag_plan.to_string().contains("annotations_tags"),
        "{tag_plan:#}"
    );
    assert_eq!(library_plan[0]["Plan"]["Actual Rows"], 1);
    assert_eq!(tag_plan[0]["Plan"]["Actual Rows"], 1);
    assert_eq!(
        store::matches(&pool, "needle", None, 50)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    println!(
        "10000 annotated matches: {} ms for text, {} ms for exact tag",
        library_plan[0]["Execution Time"], tag_plan[0]["Execution Time"]
    );
    // An unannotated library must retain its original metadata GIN access path.
    sqlx::query("TRUNCATE annotations")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ANALYZE annotations")
        .execute(&pool)
        .await
        .unwrap();
    let unannotated_plan: Value = sqlx::query_scalar(library_sql)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        unannotated_plan.to_string().contains("matches_search_trgm"),
        "{unannotated_plan:#}"
    );
    assert_eq!(unannotated_plan[0]["Plan"]["Actual Rows"], 1);
}
