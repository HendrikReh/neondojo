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
const NEEDLE: &str = "4HPwATDgc/ABMA";
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
async fn seed(pool: &PgPool) -> (Uuid, Uuid, Vec<Uuid>) {
    let mut expected = Vec::new();
    let db_a = Uuid::from_u128(10);
    let db_b = Uuid::from_u128(20);
    for (db, name, match_id, count) in [
        (db_a, "Occurrence A", Uuid::from_u128(100), 55),
        (db_b, "Occurrence B", Uuid::from_u128(200), 2),
    ] {
        sqlx::query("INSERT INTO databases(id,name) VALUES($1,$2)")
            .bind(db)
            .bind(name)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,database_id,display_name,created_at) VALUES($1,'repeat.sgf','sgf',$2,'Repeating','Opponent',7,$3,''::bytea,'',$4,'Same','2026-01-01'::timestamptz)").bind(match_id).bind(match_id.to_string()).bind(count).bind(db).execute(pool).await.unwrap();
        for n in 0..count {
            let id = Uuid::new_v4();
            expected.push(id);
            sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) VALUES($1,$2,$3,1,$4,0,ARRAY[3,1],array_fill(0,ARRAY[26]),$5,'cInlAAAAAAAE','8/5 6/5')").bind(id).bind(match_id).bind(n).bind(n*2+1).bind(NEEDLE).execute(pool).await.unwrap();
        }
    }
    (db_a, db_b, expected)
}
#[sqlx::test(migrations = "./migrations")]
async fn every_occurrence_has_its_exact_identity_and_paginate_within_a_match(pool: PgPool) {
    let (db_a, _, expected) = seed(&pool).await;
    let app = api::router(AppState::new(pool, Engine::new("/unavailable".into())));
    for (sort, direction) in [
        ("name", "asc"),
        ("name", "desc"),
        ("played_at", "asc"),
        ("played_at", "desc"),
        ("tags", "asc"),
        ("tags", "desc"),
        ("comment", "asc"),
        ("comment", "desc"),
        ("id", "desc"),
    ] {
        let mut path = format!(
            "/api/library/matches?position_id={NEEDLE}&sort={sort}&direction={direction}&limit=7"
        );
        let mut found = Vec::new();
        loop {
            let (status, page) = get(&app, &path).await;
            assert_eq!(status, 200, "{page}");
            for row in page["items"].as_array().unwrap() {
                let occurrence = &row["occurrence"];
                assert!(
                    occurrence.is_object(),
                    "position results must represent individual occurrences: {row}"
                );
                assert_eq!(occurrence["game_no"], 1);
                assert!(
                    occurrence["database_name"]
                        .as_str()
                        .unwrap()
                        .starts_with("Occurrence ")
                );
                found.push(occurrence["id"].as_str().unwrap().parse::<Uuid>().unwrap());
            }
            assert!(
                found.len() <= expected.len(),
                "pagination must terminate without duplicate occurrences"
            );
            let Some(cursor) = page["next_cursor"].as_str() else {
                break;
            };
            assert!(cursor.len() < 4096);
            path = format!(
                "/api/library/matches?position_id={NEEDLE}&sort={sort}&direction={direction}&limit=7&after={cursor}"
            );
        }
        let ordered = if sort == "id" {
            [expected[55..].to_vec(), expected[..55].to_vec()].concat()
        } else {
            expected.clone()
        };
        assert_eq!(
            found, ordered,
            "sort {sort}: do not collapse repeated positions or skip remaining occurrences in a match"
        );
    }
    let (_, scoped) = get(
        &app,
        &format!("/api/library/matches?position_id={NEEDLE}&database_id={db_a}&limit=100"),
    )
    .await;
    assert_eq!(scoped["items"].as_array().unwrap().len(), 55);
    let (_, ordinary) = get(&app, "/api/library/matches").await;
    assert_eq!(ordinary["items"].as_array().unwrap().len(), 2);
    assert!(
        ordinary["items"][0]
            .get("occurrence")
            .is_none_or(Value::is_null)
    );
    let (_, page) = get(
        &app,
        &format!("/api/library/matches?position_id={NEEDLE}&sort=id&direction=asc&limit=2"),
    )
    .await;
    let second = &page["items"][1]["occurrence"];
    assert_eq!(second["move_no"], 3);
    let (status, position) = get(
        &app,
        &format!("/api/positions/{}", second["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(status, 200, "{position}");
    assert_eq!(position["ply"], 3);
    let cursor = page["next_cursor"].as_str().unwrap();
    assert_eq!(get(&app,&format!("/api/library/matches?position_id={NEEDLE}&sort=id&direction=asc&limit=2&database_id={db_a}&after={cursor}")).await.0,422);
}
#[sqlx::test(migrations = "./migrations")]
async fn compact_occurrence_cursor_rejects_a_deleted_source_anchor(pool: PgPool) {
    seed(&pool).await;
    let app = api::router(AppState::new(
        pool.clone(),
        Engine::new("/unavailable".into()),
    ));
    let (_, page) = get(
        &app,
        &format!("/api/library/matches?position_id={NEEDLE}&sort=comment&limit=1"),
    )
    .await;
    let id: Uuid = page["items"][0]["occurrence"]["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    sqlx::query("DELETE FROM positions WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get(
            &app,
            &format!(
                "/api/library/matches?position_id={NEEDLE}&sort=comment&limit=1&after={}",
                page["next_cursor"].as_str().unwrap()
            )
        )
        .await
        .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn occurrence_columns_sort_all_occurrences_before_paginating(pool: PgPool) {
    seed(&pool).await;
    sqlx::query(
        "UPDATE databases SET name=CASE WHEN id=$1 THEN 'zulu' ELSE 'Alpha' END WHERE id=ANY($2)",
    )
    .bind(Uuid::from_u128(10))
    .bind(vec![Uuid::from_u128(10), Uuid::from_u128(20)])
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE positions SET game_no=(ARRAY[10,2,1])[ordinal % 3 + 1]")
        .execute(&pool)
        .await
        .unwrap();
    let raw: Vec<(Uuid,String,i32,i32,Uuid,i32)> = sqlx::query_as("SELECT p.id,d.name,p.game_no,p.ply,p.match_uuid,p.ordinal FROM positions p JOIN matches m ON m.id=p.match_uuid JOIN databases d ON d.id=m.database_id").fetch_all(&pool).await.unwrap();
    let app = api::router(AppState::new(pool, Engine::new("/unavailable".into())));
    for sort in ["occurrence_database", "occurrence_game", "occurrence_move"] {
        for direction in ["asc", "desc"] {
            let mut expected = raw.clone();
            expected.sort_by(|a, b| {
                let key = match sort {
                    "occurrence_database" => a.1.to_lowercase().cmp(&b.1.to_lowercase()),
                    "occurrence_game" => a.2.cmp(&b.2),
                    _ => a.3.cmp(&b.3),
                };
                let key = if direction == "desc" {
                    key.reverse()
                } else {
                    key
                };
                key.then(a.4.cmp(&b.4))
                    .then(a.5.cmp(&b.5))
                    .then(a.0.cmp(&b.0))
            });
            let base = format!(
                "/api/library/matches?position_id={NEEDLE}&sort={sort}&direction={direction}&limit=7"
            );
            let mut path = base.clone();
            let mut found = Vec::new();
            loop {
                let (status, page) = get(&app, &path).await;
                assert_eq!(status, 200, "{page}");
                found.extend(page["items"].as_array().unwrap().iter().map(|r| {
                    r["occurrence"]["id"]
                        .as_str()
                        .unwrap()
                        .parse::<Uuid>()
                        .unwrap()
                }));
                assert!(
                    found.len() <= raw.len(),
                    "pagination must terminate without duplicates"
                );
                let Some(cursor) = page["next_cursor"].as_str() else {
                    break;
                };
                path = format!("{base}&after={cursor}");
            }
            assert_eq!(
                found,
                expected.iter().map(|r| r.0).collect::<Vec<_>>(),
                "{sort} {direction}"
            );
        }
        assert_eq!(
            get(&app, &format!("/api/library/matches?sort={sort}"))
                .await
                .0,
            422,
            "occurrence sorts require a position filter"
        );
    }
}
