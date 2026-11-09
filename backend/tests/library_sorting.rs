use axum::{body::Body, http::Request};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bgtutor::{
    api::{self, AppState},
    engine::Engine,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn request(pool: &PgPool, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let response = api::router(AppState::new(pool.clone(), Engine::new("unused".into())))
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(
                    body.map(|v| Body::from(v.to_string()))
                        .unwrap_or_else(Body::empty),
                )
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(json!({"bytes":bytes.len()})),
    )
}

async fn target(pool: &PgPool, n: u128) -> Uuid {
    let id = Uuid::from_u128(n);
    sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf) VALUES($1,$2,'sgf',$3,'Alice','Bob',7,1,'original'::bytea,'(;DT[2025-04-03]EV[Open]RE[W+7])')")
        .bind(id).bind(format!("Match {n}.sgf")).bind(id.to_string()).execute(pool).await.unwrap();
    id
}

async fn ordered(pool: &PgPool, sort: &str, direction: &str, limit: usize) -> Vec<Uuid> {
    let base = format!("/api/library/matches?sort={sort}&direction={direction}&limit={limit}");
    let mut after = String::new();
    let mut ids = Vec::new();
    loop {
        let (status, page) = request(pool, "GET", &format!("{base}{after}"), None).await;
        assert_eq!(status, 200, "{base}: {page}");
        for item in page["items"].as_array().unwrap() {
            ids.push(Uuid::parse_str(item["id"].as_str().unwrap()).unwrap());
        }
        let Some(cursor) = page["next_cursor"].as_str() else {
            break;
        };
        assert!(ids.len() < 100, "pagination did not terminate");
        assert!(
            cursor.len() < 4096,
            "oversized {sort} cursor: {}",
            cursor.len()
        );
        after = format!("&after={cursor}");
    }
    ids
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn numeric_ids_are_persistent_and_independent_of_uuid_and_row_order(pool: PgPool) {
    let mut ids = Vec::new();
    for n in (1..=12).rev() {
        ids.push(target(&pool, n).await);
    }
    let (status, page) = request(
        &pool,
        "GET",
        "/api/library/matches?sort=id&direction=asc",
        None,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    let numbers: Vec<_> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["match_no"].as_i64().expect("numeric match_no"))
        .collect();
    assert!(numbers.iter().all(|n| *n > 0));
    assert!(numbers.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(ordered(&pool, "id", "asc", 2).await, ids);
    assert_eq!(
        ordered(&pool, "id", "desc", 1).await,
        ids.iter().copied().rev().collect::<Vec<_>>()
    );
    let original = ids[0];
    let original_no = numbers[0];
    let (status, renamed) = request(
        &pool,
        "PUT",
        &format!("/api/matches/{original}/metadata"),
        Some(json!({"version":0,"display_name":"Renamed","result_state":"unknown"})),
    )
    .await;
    assert_eq!(status, 200, "{renamed}");
    assert_eq!(renamed["match_no"], original_no);
    let db = Uuid::new_v4();
    sqlx::query("INSERT INTO databases(id,name) VALUES($1,'Target')")
        .bind(db)
        .execute(&pool)
        .await
        .unwrap();
    let (status, moved) = request(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"move","match_ids":[original],"target_database_id":db})),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(moved["results"][0]["status"], "moved");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT match_no FROM matches WHERE id=$1")
            .bind(original)
            .fetch_one(&pool)
            .await
            .unwrap(),
        original_no
    );
    let source = ids[1];
    let (status, copied) = request(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"copy","match_ids":[source],"target_database_id":db})),
    )
    .await;
    assert_eq!(status, 200, "{copied}");
    let copy = Uuid::parse_str(copied["results"][0]["created_id"].as_str().unwrap()).unwrap();
    let rows: Vec<(Uuid, i64, Vec<u8>, String)> = sqlx::query_as(
        "SELECT id,match_no,original,sgf FROM matches WHERE id=ANY($1) ORDER BY match_no",
    )
    .bind(vec![source, copy])
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows[0].0, source);
    assert_eq!(rows[1].0, copy);
    assert!(rows[1].1 > *numbers.last().unwrap());
    assert_eq!((&rows[0].2, &rows[0].3), (&rows[1].2, &rows[1].3));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn new_sort_fields_use_visible_values_ties_and_nulls_last_on_every_page(pool: PgPool) {
    let mut ids = Vec::new();
    for n in 1..=5 {
        ids.push(target(&pool, n).await);
    }
    for (i, id) in ids.iter().enumerate() {
        let known = i < 3;
        sqlx::query("UPDATE matches SET format=$2,position_count=$3,tournament=$4,white_score=$5,black_score=$6 WHERE id=$1")
            .bind(id).bind(if i < 2 { "mat" } else if i == 2 { "sgf" } else { "xg" })
            .bind(if i < 2 { 9 } else if i == 2 { 11 } else { 0 })
            .bind(if i == 0 { Some("Alpha") } else if i == 1 { Some("aLPHa") } else if i == 2 { Some("Beta") } else { None })
            .bind(if known { Some(if i == 2 {11} else {9}) } else {None::<i32>})
            .bind(if known { Some(if i == 2 {9} else {11}) } else {None::<i32>})
            .execute(&pool).await.unwrap();
        if known {
            sqlx::query(
                "INSERT INTO annotations(id,match_uuid,tags,comment,version) VALUES($1,$2,$3,$4,1)",
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(vec![if i == 0 {
                "Alpha"
            } else if i == 1 {
                "aLPHa"
            } else {
                "Beta"
            }])
            .bind(if i == 0 {
                "Alpha"
            } else if i == 1 {
                "aLPHa"
            } else {
                "Beta"
            })
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO jobs(id,match_uuid,kind,plies,status) VALUES($1,$2,'match',0,$3)",
            )
            .bind(Uuid::new_v4())
            .bind(id)
            .bind(if i < 2 { "queued" } else { "running" })
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    // An explicitly empty annotation also counts as a missing visible value.
    sqlx::query(
        "INSERT INTO annotations(id,match_uuid,tags,comment,version) VALUES($1,$2,'{}','',1)",
    )
    .bind(Uuid::new_v4())
    .bind(ids[3])
    .execute(&pool)
    .await
    .unwrap();
    // Incomplete scores must remain missing rather than gaining a partial numeric key.
    sqlx::query("UPDATE matches SET white_score=100 WHERE id=$1")
        .bind(ids[3])
        .execute(&pool)
        .await
        .unwrap();
    for sort in [
        "tournament",
        "format",
        "position_count",
        "analysis_status",
        "tags",
        "comment",
        "score",
    ] {
        for direction in ["asc", "desc"] {
            let order = match (sort, direction) {
                ("position_count", "asc") => vec![3, 4, 0, 1, 2],
                ("format", "desc") => vec![3, 4, 2, 0, 1],
                ("position_count", "desc") => vec![2, 0, 1, 3, 4],
                (_, "asc") => vec![0, 1, 2, 3, 4],
                (_, "desc") => vec![2, 0, 1, 3, 4],
                _ => unreachable!(),
            };
            let expected: Vec<_> = order.into_iter().map(|i| ids[i]).collect();
            for limit in [1, 2, 200] {
                assert_eq!(
                    ordered(&pool, sort, direction, limit).await,
                    expected,
                    "{sort} {direction}, limit {limit}"
                );
            }
        }
    }
    // Existing result-state sorting remains available alongside the score column.
    assert_eq!(ordered(&pool, "result", "asc", 1).await, ids);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn analysis_sort_uses_displayed_latest_partial_and_imported_status(pool: PgPool) {
    let mut ids = Vec::new();
    for n in 1..=7 {
        ids.push(target(&pool, n).await);
    }
    for (i, status) in ["completed", "failed", "partial", "queued", "running"]
        .into_iter()
        .enumerate()
    {
        let id = ids[if i < 2 { i } else { i + 1 }];
        sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,game_no,created_at) VALUES($1,$2,'match',0,$3,1,$4,now()-interval '1 hour')")
            .bind(Uuid::new_v4()).bind(id).bind(if status == "partial" {"completed"} else {status}).bind(if status == "partial" {Some(1)} else {None::<i32>}).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO match_games(match_uuid,game_no,position_count,white_score,black_score,crawford) VALUES($1,1,0,0,0,false)").bind(ids[2]).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO imported_analyses(match_uuid,source_ordinal,game_no,source_move_no,kind,data) VALUES($1,0,1,0,'checker','{}')").bind(ids[2]).execute(&pool).await.unwrap();
    // The displayed whole-match job outranks a newer per-game job.
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,game_no) VALUES($1,$2,'match',1,'failed',1,1)").bind(Uuid::new_v4()).bind(ids[0]).execute(&pool).await.unwrap();
    assert_eq!(ordered(&pool, "analysis_status", "asc", 1).await, ids);
    let mut descending = ids[..6].to_vec();
    descending.reverse();
    descending.push(ids[6]);
    assert_eq!(
        ordered(&pool, "analysis_status", "desc", 1).await,
        descending
    );
}

fn altered_cursor(cursor: &str, field: &str, value: Value) -> String {
    let mut value_cursor: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(cursor).unwrap()).unwrap();
    value_cursor[field] = value;
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(&value_cursor).unwrap())
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn full_unicode_annotations_use_compact_scope_bound_anchor_cursors(pool: PgPool) {
    let a = target(&pool, 1).await;
    let b = target(&pool, 2).await;
    let outside = target(&pool, 3).await;
    sqlx::query("UPDATE matches SET tournament=CASE WHEN id=$1 THEN 'Outside' ELSE 'Final' END")
        .bind(outside)
        .execute(&pool)
        .await
        .unwrap();
    for (i, id) in [a, b].into_iter().enumerate() {
        let comment = format!(
            "{}{}",
            if i == 0 { "a" } else { "b" },
            "界🦊".repeat(10_000)
        );
        let comment: String = comment.chars().take(20_000).collect();
        let tags: Vec<_> = (0..30)
            .map(|n| format!("{n:02}{}", "界".repeat(62)))
            .collect();
        sqlx::query(
            "INSERT INTO annotations(id,match_uuid,tags,comment,version) VALUES($1,$2,$3,$4,1)",
        )
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(tags)
        .bind(comment)
        .execute(&pool)
        .await
        .unwrap();
    }
    for sort in ["tags", "comment"] {
        for direction in ["asc", "desc"] {
            let base = format!(
                "/api/library/matches?sort={sort}&direction={direction}&tournament=Final&limit=1"
            );
            let (status, first) = request(&pool, "GET", &base, None).await;
            assert_eq!(status, 200, "{first}");
            let cursor = first["next_cursor"].as_str().unwrap();
            assert!(cursor.len() < 512, "{sort}: {}", cursor.len());
            let (status, next) =
                request(&pool, "GET", &format!("{base}&after={cursor}"), None).await;
            assert_eq!(status, 200, "{next}");
            assert_eq!(next["items"].as_array().unwrap().len(), 1);
            assert_ne!(next["items"][0]["id"], first["items"][0]["id"]);
            for (field, value) in [
                ("anchor", json!(false)),
                ("anchor", json!("invalid")),
                ("key", json!("forged annotation value")),
                ("id", json!("invalid UUID")),
            ] {
                let malformed = altered_cursor(cursor, field, value);
                assert_eq!(
                    request(&pool, "GET", &format!("{base}&after={malformed}"), None)
                        .await
                        .0,
                    422,
                    "malformed {sort} cursor field {field}"
                );
            }
            for changed in [
                format!("sort={sort}&direction={direction}&tournament=Outside"),
                format!("sort={sort}&direction={direction}&tournament=Final&player=Bob"),
                format!(
                    "sort={sort}&direction={}&tournament=Final",
                    if direction == "asc" { "desc" } else { "asc" }
                ),
            ] {
                assert_eq!(
                    request(
                        &pool,
                        "GET",
                        &format!("/api/library/matches?{changed}&after={cursor}"),
                        None
                    )
                    .await
                    .0,
                    422
                );
            }
            for id in [Uuid::new_v4(), outside] {
                let forged = altered_cursor(cursor, "id", json!(id));
                assert_eq!(
                    request(&pool, "GET", &format!("{base}&after={forged}"), None)
                        .await
                        .0,
                    422,
                    "missing or out-of-scope anchor"
                );
            }
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn numeric_cursor_validation_rejects_bad_bigints_and_score_arrays(pool: PgPool) {
    target(&pool, 1).await;
    target(&pool, 2).await;
    sqlx::query("UPDATE matches SET white_score=9,black_score=11")
        .execute(&pool)
        .await
        .unwrap();
    for (sort, invalid) in [
        (
            "id",
            vec![json!("9223372036854775808"), json!("1.5"), Value::Null],
        ),
        (
            "score",
            vec![
                json!("{9,x}"),
                json!("{9}"),
                json!("{9,11,12}"),
                json!("{9,-1}"),
                json!("{9,2147483648}"),
                json!("{NULL,11}"),
            ],
        ),
    ] {
        let base = format!("/api/library/matches?sort={sort}&limit=1");
        let (status, first) = request(&pool, "GET", &base, None).await;
        assert_eq!(status, 200, "{first}");
        let cursor = first["next_cursor"].as_str().unwrap();
        for value in invalid {
            let forged = altered_cursor(cursor, "key", value.clone());
            assert_eq!(
                request(&pool, "GET", &format!("{base}&after={forged}"), None)
                    .await
                    .0,
                422,
                "{sort}: {value}"
            );
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn score_sort_compares_each_side_numerically_before_the_uuid_tie_breaker(pool: PgPool) {
    let low_white = target(&pool, 4).await;
    let low_black = target(&pool, 3).await;
    let high_black = target(&pool, 2).await;
    let high_white = target(&pool, 1).await;
    for (id, white, black) in [
        (low_white, 8, 11),
        (low_black, 9, 9),
        (high_black, 9, 11),
        (high_white, 11, 0),
    ] {
        sqlx::query("UPDATE matches SET white_score=$2,black_score=$3 WHERE id=$1")
            .bind(id)
            .bind(white)
            .bind(black)
            .execute(&pool)
            .await
            .unwrap();
    }
    let expected = vec![low_white, low_black, high_black, high_white];
    assert_eq!(ordered(&pool, "score", "asc", 1).await, expected);
    assert_eq!(
        ordered(&pool, "score", "desc", 1).await,
        expected.into_iter().rev().collect::<Vec<_>>()
    );
}

#[sqlx::test(migrations = false)]
#[ignore = "requires PostgreSQL"]
async fn migration_backfills_existing_rows_without_changing_source_data(pool: PgPool) {
    let all = sqlx::migrate!("./migrations");
    let before = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter().filter(|m| m.version < 21).cloned().collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    before.run(&pool).await.unwrap();
    let a = target(&pool, 1).await;
    let b = target(&pool, 2).await;
    let original: Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(m) FROM matches m ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    all.run(&pool).await.unwrap();
    let (status, page) = request(
        &pool,
        "GET",
        "/api/library/matches?sort=id&direction=asc",
        None,
    )
    .await;
    assert_eq!(status, 200, "{page}");
    let numbers: Vec<_> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["match_no"].as_i64().expect("backfilled match_no"))
        .collect();
    assert_eq!(numbers.len(), 2);
    assert!(numbers[0] > 0 && numbers[1] > numbers[0]);
    assert_eq!(
        sqlx::query_scalar::<_, Value>("SELECT to_jsonb(m)-ARRAY['match_no','collection_only','source_match_id','source_match_no','source_database_id','source_database_name'] FROM matches m ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap(),
        original
    );
    let c = target(&pool, 3).await;
    assert!(
        sqlx::query_scalar::<_, i64>("SELECT match_no FROM matches WHERE id=$1")
            .bind(c)
            .fetch_one(&pool)
            .await
            .unwrap()
            > numbers[1]
    );
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM matches WHERE id=ANY($1) ORDER BY id")
            .bind(vec![a, b])
            .fetch_all(&pool)
            .await
            .unwrap(),
        vec![a, b]
    );
    assert!(
        sqlx::query("UPDATE matches SET match_no=1 WHERE id=$1")
            .bind(c)
            .execute(&pool)
            .await
            .is_err(),
        "identity is generated always"
    );
}
