use axum::{body::Body, http::Request};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
    import,
    model::Position,
    store,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const SOURCE: &str = "(;GM[6]FF[4]PW[Mochy]PB[Opponent]MI[length:7][ws:0][bs:0];W[31hefe])\n(;GM[6]FF[4]MI[length:7][ws:2][bs:3];B[31qtst])";

fn app(pool: &PgPool) -> axum::Router {
    api::router(AppState::new(pool.clone(), Engine::new("unused".into())))
}

async fn request(pool: &PgPool, method: &str, path: &str) -> (u16, Value) {
    let response = app(pool)
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn bookmark(id: Uuid) -> String {
    format!("/api/positions/{id}/bookmark")
}

async fn imported(pool: &PgPool) -> (Uuid, Vec<Position>) {
    let imported = store::import_match(
        pool,
        "saved.sgf",
        "sgf",
        SOURCE.as_bytes(),
        SOURCE,
        &import::parse_sgf(SOURCE).unwrap(),
    )
    .await
    .unwrap();
    let id = imported.r#match.id;
    let positions = sqlx::query_as("SELECT * FROM positions WHERE match_uuid=$1 ORDER BY ordinal")
        .bind(id)
        .fetch_all(pool)
        .await
        .unwrap();
    (id, positions)
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn bookmark_save_is_explicit_idempotent_and_persists_after_reopening(pool: PgPool) {
    let (_, positions) = imported(&pool).await;
    let id = positions[0].id;
    let path = bookmark(id);
    let (status, initial) = request(&pool, "GET", &path).await;
    assert_eq!(status, 200, "{initial}");
    assert_eq!(
        initial,
        json!({"position_id":id,"saved":false,"saved_at":null})
    );
    let (status, saved) = request(&pool, "PUT", &path).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["position_id"], id.to_string());
    assert_eq!(saved["saved"], true);
    chrono::DateTime::parse_from_rfc3339(saved["saved_at"].as_str().unwrap()).unwrap();
    assert_eq!(request(&pool, "PUT", &path).await, (200, saved.clone()));
    assert_eq!(request(&pool, "GET", &path).await, (200, saved));
    assert_eq!(
        request(&pool, "GET", &bookmark(positions[1].id)).await.1["saved"],
        false
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn simultaneous_saves_retain_one_original_timestamp(pool: PgPool) {
    let (_, positions) = imported(&pool).await;
    let path = bookmark(positions[0].id);
    let (one, two) = tokio::join!(request(&pool, "PUT", &path), request(&pool, "PUT", &path));
    assert_eq!(one.0, 200, "{one:?}");
    assert_eq!(two, one);
    assert_eq!(request(&pool, "GET", &path).await, one);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn removal_is_idempotent_and_retains_source_notes_analysis_and_learning_card(pool: PgPool) {
    let (match_id, positions) = imported(&pool).await;
    let id = positions[0].id;
    let path = bookmark(id);
    sqlx::query("INSERT INTO annotations(id,position_uuid,tags,comment,version) VALUES($1,$2,ARRAY['study'],'Keep this note',1)").bind(Uuid::new_v4()).bind(id).execute(&pool).await.unwrap();
    let card = Uuid::new_v4();
    sqlx::query("INSERT INTO learning_cards(id,signature,payload) VALUES($1,$2,$3)")
        .bind(card)
        .bind(id.to_string())
        .bind(json!({"position_id":id,"source_match_id":match_id}))
        .execute(&pool)
        .await
        .unwrap();
    let job = store::create_job(&pool, id, "analysis", 1).await.unwrap();
    let source = store::get_position(&pool, id).await.unwrap().unwrap();
    assert_eq!(request(&pool, "PUT", &path).await.0, 200);
    assert_eq!(request(&pool, "DELETE", &path).await, (204, Value::Null));
    assert_eq!(request(&pool, "DELETE", &path).await, (204, Value::Null));
    assert_eq!(
        request(&pool, "GET", &path).await.1,
        json!({"position_id":id,"saved":false,"saved_at":null})
    );
    assert_eq!(
        serde_json::to_value(store::get_position(&pool, id).await.unwrap().unwrap()).unwrap(),
        serde_json::to_value(source).unwrap()
    );
    assert!(store::get_match(&pool, match_id).await.unwrap().is_some());
    assert!(store::get_job(&pool, job.id).await.unwrap().is_some());
    let notes: String =
        sqlx::query_scalar("SELECT comment FROM annotations WHERE position_uuid=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(notes, "Keep this note");
    let payload: Value = sqlx::query_scalar("SELECT payload FROM learning_cards WHERE id=$1")
        .bind(card)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payload["position_id"], id.to_string());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn identical_checker_boards_in_opposite_colors_are_saved_as_exact_occurrences(pool: PgPool) {
    let (_, positions) = imported(&pool).await;
    assert_eq!(positions.len(), 2);
    assert_eq!(positions[0].position_id, positions[1].position_id);
    assert_ne!(positions[0].player, positions[1].player);
    for position in &positions {
        let (status, saved) = request(&pool, "PUT", &bookmark(position.id)).await;
        assert_eq!(status, 200, "{saved}");
        assert_eq!(saved["position_id"], position.id.to_string());
    }
    assert_eq!(
        request(&pool, "DELETE", &bookmark(positions[0].id)).await.0,
        204
    );
    assert_eq!(
        request(&pool, "GET", &bookmark(positions[1].id)).await.1["saved"],
        true
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn unknown_and_invalid_position_ids_do_not_create_membership(pool: PgPool) {
    for method in ["GET", "PUT", "DELETE"] {
        assert_eq!(
            request(&pool, method, &bookmark(Uuid::new_v4())).await.0,
            404
        );
        let (status, body) = request(&pool, method, "/api/positions/not-a-uuid/bookmark").await;
        assert_eq!(status, 422, "{body}");
        assert!(body["error"].is_string(), "{body}");
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn deleting_the_source_removes_only_its_saved_occurrences(pool: PgPool) {
    let (match_id, positions) = imported(&pool).await;
    for position in &positions {
        assert_eq!(request(&pool, "PUT", &bookmark(position.id)).await.0, 200);
    }
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap();
    for position in &positions {
        assert_eq!(request(&pool, "GET", &bookmark(position.id)).await.0, 404);
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM saved_positions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn bookmark_operations_wait_for_source_deletion_and_then_return_not_found(pool: PgPool) {
    let (match_id, positions) = imported(&pool).await;
    let id = positions[0].id;
    let mut deletion = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM matches WHERE id=$1 FOR UPDATE")
        .bind(match_id)
        .execute(&mut *deletion)
        .await
        .unwrap();
    let mut submissions = Vec::new();
    for method in ["GET", "PUT", "DELETE"] {
        let submit_pool = pool.clone();
        submissions.push(tokio::spawn(async move {
            request(&submit_pool, method, &bookmark(id)).await
        }));
    }
    for submission in &mut submissions {
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), submission)
                .await
                .is_err(),
            "bookmark operation must respect match deletion lock"
        );
    }
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(match_id)
        .execute(&mut *deletion)
        .await
        .unwrap();
    deletion.commit().await.unwrap();
    for submission in submissions {
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(2), submission)
                .await
                .unwrap()
                .unwrap()
                .0,
            404
        );
    }
}

#[test]
fn openapi_documents_bookmark_contract() {
    use utoipa::OpenApi;
    let doc = serde_json::to_value(api::ApiDoc::openapi()).unwrap();
    let path = &doc["paths"]["/api/positions/{id}/bookmark"];
    assert_eq!(
        path["get"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/PositionBookmark"
    );
    assert!(path["put"]["responses"]["200"].is_object());
    assert!(path["delete"]["responses"]["204"].is_object());
    for method in ["get", "put", "delete"] {
        assert!(path[method]["responses"]["404"].is_object());
        assert!(path[method]["responses"]["422"].is_object());
    }
}

fn encoded(value: &Value) -> String {
    value
        .to_string()
        .bytes()
        .map(|byte| format!("%{byte:02X}"))
        .collect()
}

async fn browser(pool: &PgPool, fields: Value, after: Option<&str>, limit: usize) -> (u16, Value) {
    let mut definition = fields;
    definition["schema_version"] = json!(1);
    let mut uri = format!(
        "/api/position-browser?filters={}&limit={limit}",
        encoded(&definition)
    );
    if let Some(after) = after {
        uri.push_str(&format!("&after={after}"));
    }
    request(pool, "GET", &uri).await
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn imported_positions_notes_analysis_and_cards_never_implicitly_enter_the_collection(
    pool: PgPool,
) {
    let (match_id, positions) = imported(&pool).await;
    let id = positions[0].id;
    sqlx::query("INSERT INTO annotations(id,position_uuid,tags,comment,version) VALUES($1,$2,ARRAY['study'],'interesting decision',1)").bind(Uuid::new_v4()).bind(id).execute(&pool).await.unwrap();
    let job = store::create_job(&pool, id, "analysis", 1).await.unwrap();
    sqlx::query("UPDATE jobs SET status='completed',result=$2 WHERE id=$1")
        .bind(job.id)
        .bind(json!({"candidates":[{"notation":"8/5 6/5"}]}))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO learning_cards(id,signature,payload) VALUES($1,$2,$3)")
        .bind(Uuid::new_v4())
        .bind(id.to_string())
        .bind(json!({"position_id":id}))
        .execute(&pool)
        .await
        .unwrap();
    for fields in [
        json!({}),
        json!({"tag":"study"}),
        json!({"comment":"interesting"}),
        json!({"analysis_state":"ready"}),
        json!({"player":"Mochy"}),
        json!({"position_id":positions[0].position_id}),
    ] {
        let (status, page) = browser(&pool, fields, None, 1).await;
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["items"], json!([]), "{page}");
        assert!(page["next_cursor"].is_null());
    }
    let (status, source) =
        request(&pool, "GET", &format!("/api/positions?match_id={match_id}")).await;
    assert_eq!(status, 200, "{source}");
    assert_eq!(source["items"].as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn every_browser_sort_and_direction_lists_only_explicit_saved_occurrences(pool: PgPool) {
    let (_, positions) = imported(&pool).await;
    let id = positions[1].id;
    assert_eq!(request(&pool, "PUT", &bookmark(id)).await.0, 200);
    for sort in [
        "match",
        "loss",
        "match_id",
        "database",
        "game",
        "ply",
        "player",
        "opponent",
        "dice",
        "score",
        "cube",
        "played_move",
        "tags",
        "comment",
    ] {
        for direction in [None, Some("asc"), Some("desc")] {
            let mut fields = json!({"sort":sort});
            if let Some(direction) = direction {
                fields["direction"] = json!(direction);
            }
            let (status, page) = browser(&pool, fields, None, 1).await;
            assert_eq!(status, 200, "{page}");
            assert_eq!(page["items"].as_array().unwrap().len(), 1, "{page}");
            assert_eq!(page["items"][0]["position"]["id"], id.to_string());
            assert_eq!(page["items"][0]["position"]["player"], positions[1].player);
            assert!(page["next_cursor"].is_null());
        }
    }
    assert_eq!(request(&pool, "DELETE", &bookmark(id)).await.0, 204);
    assert_eq!(
        browser(&pool, json!({}), None, 25).await.1["items"],
        json!([])
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn all_browser_cursor_forms_reject_removed_and_unsaved_anchors(pool: PgPool) {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    let (_, positions) = imported(&pool).await;
    for fields in [
        json!({"sort":"match"}),
        json!({"sort":"loss"}),
        json!({"sort":"player","direction":"desc"}),
    ] {
        for position in &positions {
            assert_eq!(request(&pool, "PUT", &bookmark(position.id)).await.0, 200);
        }
        let (status, first) = browser(&pool, fields.clone(), None, 1).await;
        assert_eq!(status, 200, "{first}");
        let cursor = first["next_cursor"].as_str().unwrap();
        assert_eq!(browser(&pool, fields.clone(), Some(cursor), 1).await.0, 200);
        let id = Uuid::parse_str(first["items"][0]["position"]["id"].as_str().unwrap()).unwrap();
        assert_eq!(request(&pool, "DELETE", &bookmark(id)).await.0, 204);
        let (status, body) = browser(&pool, fields.clone(), Some(cursor), 1).await;
        assert_eq!(status, 422, "Removed anchor accepted for {fields}: {body}");
        let mut forged: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(cursor).unwrap()).unwrap();
        let key = if forged.get("anchor").is_some() {
            "anchor"
        } else {
            "id"
        };
        forged[key] = json!(Uuid::new_v4());
        let forged = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&forged).unwrap());
        assert_eq!(browser(&pool, fields, Some(&forged), 1).await.0, 422);
    }
}
