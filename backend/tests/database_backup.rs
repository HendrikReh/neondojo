use axum::{
    body::{Body, Bytes, to_bytes},
    http::{Request, StatusCode, header},
};
use bgtutor::{
    api::{AppState, router},
    engine::Engine,
};
use sqlx::PgPool;
use tower::ServiceExt;

const BACKUP: &str = "/api/database-backup";
const CHECK: &str = "/api/database-backup/check";
const MEDIA_TYPE: &str = "application/vnd.bgtutor.backup";

fn restore(file: impl Into<Body>, confirm: bool) -> Request<Body> {
    Request::post(format!("{BACKUP}?replace={confirm}"))
        .header(header::CONTENT_TYPE, MEDIA_TYPE)
        .body(file.into())
        .unwrap()
}

fn check(file: impl Into<Body>) -> Request<Body> {
    Request::post(CHECK)
        .header(header::CONTENT_TYPE, MEDIA_TYPE)
        .body(file.into())
        .unwrap()
}

async fn check_result(
    app: &axum::Router,
    file: impl Into<Body>,
) -> (StatusCode, serde_json::Value) {
    let response = app.clone().oneshot(check(file)).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 16 * 1024).await.unwrap();
    (
        status,
        serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null),
    )
}

async fn insert_match(pool: &PgPool, name: &str) -> (uuid::Uuid, i64) {
    sqlx::query_as(
        "INSERT INTO matches (id,filename,format,sha256,white,black,match_length,position_count,original,sgf,source_metadata_loaded)
         VALUES(gen_random_uuid(),$1,'sgf',$1,'Héndrik','Opponent',7,0,$2,'(;GM[6])',true)
         RETURNING id,match_no",
    )
    .bind(name)
    .bind(&[0_u8, 1, 255][..])
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn restores_data_and_keeps_target_installation_settings(pool: PgPool) {
    let app = router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    let (source_id, source_number) = insert_match(&pool, "source-ä.sgf").await;
    sqlx::query("INSERT INTO annotations(id,match_uuid,tags,comment,version) VALUES(gen_random_uuid(),$1,ARRAY['täg'],'A note with a tab' || chr(9) || 'and a newline' || chr(10),1)")
        .bind(source_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app_settings(singleton,player_name,gnubg_path) VALUES(true,'Portable name','/source/GNUbg')")
        .execute(&pool).await.unwrap();
    let previous_default: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM databases WHERE is_default")
            .fetch_one(&pool)
            .await
            .unwrap();
    let pending: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO jobs(id,match_uuid,kind,plies,status,lease_token,lease_until)
         VALUES(gen_random_uuid(),$1,'match',2,'running',gen_random_uuid(),now()+interval '1 hour')
         RETURNING id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let deck_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO decks(id,name) VALUES(gen_random_uuid(),'Portable deck') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("SELECT nextval(pg_get_serial_sequence('matches', 'match_no'))")
        .execute(&pool)
        .await
        .unwrap();
    let position: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation)
         VALUES(gen_random_uuid(),$1,1,1,1,0,ARRAY[3,1],array_fill(0,ARRAY[26]),'fixture','fixture','13/10 6/5') RETURNING id",
    )
    .bind(source_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO saved_positions(position_uuid) VALUES($1)")
        .bind(position)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO learning_cards(id,signature,payload)
         VALUES(gen_random_uuid(),'backup-card-1','{}'::jsonb),
               (gen_random_uuid(),'backup-card-2','{}'::jsonb)",
    )
    .execute(&pool)
    .await
    .unwrap();
    // A saved-position host is archived but is not a match in the library.
    sqlx::query(
        "INSERT INTO matches (id,filename,format,sha256,white,black,match_length,
             position_count,original,sgf,database_id,collection_only,source_match_id,
             source_match_no,source_database_id,source_database_name,moves_cataloged,
             imported_analysis_loaded,source_metadata_loaded)
         VALUES(gen_random_uuid(),'position host','sgf','position-host','A','B',7,0,
             ''::bytea,'','00000000-0000-0000-0000-000000000002',true,$1,$2,$3,'Fixture',
             true,true,true)",
    )
    .bind(source_id)
    .bind(source_number)
    .bind(previous_default)
    .execute(&pool)
    .await
    .unwrap();
    let before_export = chrono::Utc::now();

    let exported = app
        .clone()
        .oneshot(Request::get(BACKUP).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(exported.status(), StatusCode::OK);
    assert!(
        exported.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains(".bgtutor-backup")
    );
    let archive = to_bytes(exported.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    assert!(archive.starts_with(b"PK"));

    sqlx::query("DELETE FROM matches")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE databases DISABLE TRIGGER databases_protect_system")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE databases SET id=gen_random_uuid() WHERE is_default")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("ALTER TABLE databases ENABLE TRIGGER databases_protect_system")
        .execute(&pool)
        .await
        .unwrap();
    let target_default: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM databases WHERE is_default")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(target_default, previous_default);
    sqlx::query(&format!(
        "ALTER TABLE matches ALTER COLUMN database_id SET DEFAULT '{target_default}'::uuid"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM decks")
        .execute(&pool)
        .await
        .unwrap();
    let (target_id, _) = insert_match(&pool, "target-only.sgf").await;
    sqlx::query("UPDATE app_settings SET gnubg_path='/target/GNUbg',player_name='Target name'")
        .execute(&pool)
        .await
        .unwrap();

    let manifest: serde_json::Value = {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&archive)).unwrap();
        serde_json::from_reader(zip.by_name("manifest.json").unwrap()).unwrap()
    };
    let created_at = manifest["created_at"]
        .as_str()
        .expect("archive timestamp")
        .to_owned();
    let timestamp = chrono::DateTime::parse_from_rfc3339(&created_at).unwrap();
    assert!(timestamp >= before_export && timestamp <= chrono::Utc::now());
    let mut legacy_manifest = manifest.clone();
    legacy_manifest
        .as_object_mut()
        .unwrap()
        .remove("created_at");
    legacy_manifest["format"] = serde_json::json!(1);
    let legacy_archive = Bytes::from(rewrite_entry(
        &archive,
        "manifest.json",
        legacy_manifest.to_string().as_bytes(),
    ));
    let mut wrong_schema = manifest.clone();
    wrong_schema["schema"] = serde_json::Value::String("0".repeat(64));
    let incompatible = Bytes::from(rewrite_entry(
        &archive,
        "manifest.json",
        wrong_schema.to_string().as_bytes(),
    ));
    let mut wrong_sequences = manifest;
    wrong_sequences["sequences"] = serde_json::json!([]);
    let bad_manifest = rewrite_entry(
        &archive,
        "manifest.json",
        wrong_sequences.to_string().as_bytes(),
    );
    assert_eq!(
        check_result(&app, archive.clone()).await,
        (
            StatusCode::OK,
            serde_json::json!({
                "status":"compatible",
                "contents": {
                    "created_at": created_at,
                    "matches":1, "saved_positions":1, "learning_cards":2
                }
            })
        )
    );
    assert_eq!(
        check_result(&app, legacy_archive.clone()).await,
        (
            StatusCode::OK,
            serde_json::json!({
                "status":"compatible",
                "contents": {"matches":1, "saved_positions":1, "learning_cards":2}
            })
        )
    );
    assert_eq!(
        check_result(&app, incompatible.clone()).await,
        (
            StatusCode::OK,
            serde_json::json!({
                "status":"incompatible_schema",
                "contents": {
                    "created_at": created_at,
                    "matches":1, "saved_positions":1, "learning_cards":2
                }
            })
        )
    );
    let offset = {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&archive)).unwrap();
        let entry = zip.by_name("tables/matches.copy").unwrap();
        assert!(entry.compressed_size() > 0);
        entry.data_start() as usize
    };
    let mut damaged = archive.to_vec();
    damaged[offset] ^= 0x80;
    for (file, message) in [
        (
            Bytes::from_static(b"invalid backup"),
            "Invalid BG Tutor backup archive",
        ),
        (Bytes::from(damaged), "Backup table is damaged"),
        (
            Bytes::from(bad_manifest),
            "Backup contains data that cannot be restored",
        ),
    ] {
        let (status, body) = check_result(&app, file).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"], message);
    }
    assert_eq!(
        sqlx::query_as::<_, (uuid::Uuid, String)>(
            "SELECT m.id, a.player_name FROM matches m CROSS JOIN app_settings a",
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        (target_id, "Target name".into())
    );

    for (file, confirm, expected) in [
        (archive.clone(), false, StatusCode::UNPROCESSABLE_ENTITY),
        (incompatible, true, StatusCode::UNPROCESSABLE_ENTITY),
        (
            Bytes::from_static(b"invalid backup"),
            true,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
    ] {
        assert_eq!(
            app.clone()
                .oneshot(restore(file, confirm))
                .await
                .unwrap()
                .status(),
            expected
        );
        assert_eq!(
            sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM matches")
                .fetch_one(&pool)
                .await
                .unwrap(),
            target_id
        );
    }

    assert_eq!(
        app.oneshot(restore(legacy_archive, true))
            .await
            .unwrap()
            .status(),
        StatusCode::NO_CONTENT
    );
    let restored: (uuid::Uuid, i64, Vec<u8>, String, String) = sqlx::query_as(
        "SELECT m.id,m.match_no,m.original,a.comment,p.source_name FROM matches m
         JOIN annotations a ON a.match_uuid=m.id JOIN match_players mp ON mp.match_id=m.id AND mp.side=0
         JOIN player_aliases pa ON pa.id=mp.alias_id JOIN player_profiles p ON p.id=pa.profile_id"
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(
        restored,
        (
            source_id,
            source_number,
            vec![0, 1, 255],
            "A note with a tab\tand a newline\n".into(),
            "Héndrik".into()
        )
    );
    assert_eq!(
        sqlx::query_as::<_, (String, String)>("SELECT player_name,gnubg_path FROM app_settings")
            .fetch_one(&pool)
            .await
            .unwrap(),
        ("Portable name".into(), "/target/GNUbg".into())
    );
    assert!(
        !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT FROM matches WHERE id=$1)")
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT id FROM decks WHERE name='Portable deck'")
            .fetch_one(&pool)
            .await
            .unwrap(),
        deck_id,
    );
    let (status, has_lease, latest_event): (String, bool, String) = sqlx::query_as(
        "SELECT j.status,j.lease_token IS NOT NULL,e.event FROM jobs j
         JOIN job_events e ON e.job_id=j.id WHERE j.id=$1 ORDER BY e.id DESC LIMIT 1",
    )
    .bind(pending)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (status.as_str(), has_lease, latest_event.as_str()),
        ("failed", false, "failed")
    );
    let (next_id, next_number) = insert_match(&pool, "next.sgf").await;
    assert_eq!(next_number, source_number + 3);
    assert_eq!(
        sqlx::query_scalar::<_, uuid::Uuid>("SELECT database_id FROM matches WHERE id=$1")
            .bind(next_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        previous_default,
    );
}

fn rewrite_entry(archive: &Bytes, changed_entry: &str, replacement: &[u8]) -> Vec<u8> {
    use std::io::{Cursor, Write};

    let mut source = zip::ZipArchive::new(Cursor::new(archive)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..source.len() {
        let mut entry = source.by_index(index).unwrap();
        let changed = entry.name() == changed_entry;
        output
            .start_file(entry.name(), zip::write::SimpleFileOptions::default())
            .unwrap();
        if changed {
            output.write_all(replacement).unwrap();
        } else {
            std::io::copy(&mut entry, &mut output).unwrap();
        }
    }
    output.finish().unwrap().into_inner()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn invalid_rows_roll_back_the_target_after_the_restore_starts(pool: PgPool) {
    let app = router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    insert_match(&pool, "source.sgf").await;
    let response = app
        .clone()
        .oneshot(Request::get(BACKUP).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let archive = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    sqlx::query("DELETE FROM matches")
        .execute(&pool)
        .await
        .unwrap();
    let (id, number) = insert_match(&pool, "keep.sgf").await;
    let invalid = rewrite_entry(
        &archive,
        "tables/engine_match_results.copy",
        b"invalid uuid\n",
    );
    let response = app.oneshot(restore(invalid, true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        sqlx::query_as::<_, (uuid::Uuid, i64)>("SELECT id,match_no FROM matches")
            .fetch_one(&pool)
            .await
            .unwrap(),
        (id, number),
    );
    let (next_id, next_number) = insert_match(&pool, "next.sgf").await;
    assert_eq!(next_number, number + 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM match_players WHERE match_id=$1")
            .bind(next_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn restores_anki_review_history(pool: PgPool) {
    let app = router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    let card = uuid::Uuid::new_v4();
    let session = uuid::Uuid::new_v4();
    let request = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO learning_cards(id,signature,payload,review_count,anki_review_count)
         VALUES($1,'anki-backup-card','{}',1,1)",
    )
    .bind(card)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO review_sessions(id,card_id,card_version,expires_at) VALUES($1,$2,0,now())",
    )
    .bind(session)
    .bind(card)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO card_reviews(card_id,request_id,session_id,rating,result,reviewed_at)
         VALUES($1,$2,$3,'good','{}',now())",
    )
    .bind(card)
    .bind(request)
    .bind(session)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO anki_received_reviews VALUES($1,100,'hard')")
        .bind(card)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO anki_review_exports VALUES($1,$2,200)")
        .bind(card)
        .bind(request)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .clone()
        .oneshot(Request::get(BACKUP).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let archive = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    for table in ["anki_received_reviews", "anki_review_exports"] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE learning_cards SET anki_review_count=0 WHERE id=$1")
        .bind(card)
        .execute(&pool)
        .await
        .unwrap();

    let response = app.oneshot(restore(archive, true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let restored: (i32, i32, i64, String, i64, String) = sqlx::query_as(
        "SELECT c.review_count,c.anki_review_count,received.review_id,received.rating,
                sent.review_id,r.rating
         FROM learning_cards c
         JOIN anki_received_reviews received ON received.card_id=c.id
         JOIN anki_review_exports sent ON sent.card_id=c.id
         JOIN card_reviews r ON r.card_id=sent.card_id AND r.request_id=sent.request_id
         WHERE c.id=$1 AND sent.request_id=$2",
    )
    .bind(card)
    .bind(request)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(restored, (1, 1, 100, "hard".into(), 200, "good".into()));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn restored_deck_requires_new_opt_in_for_automatic_anki_sync(pool: PgPool) {
    let app = router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    let deck: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO decks(id,name) VALUES(gen_random_uuid(),'Linked deck') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO anki_auto_sync(deck_id,link_id,profile,collection_key)
         VALUES($1,gen_random_uuid(),'Source profile','source-collection')",
    )
    .bind(deck)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO anki_sync_history(deck_id,profile,received_reviews,sent_reviews,outcome)
         VALUES($1,'Source profile',2,3,'synced')",
    )
    .bind(deck)
    .execute(&pool)
    .await
    .unwrap();
    let response = app
        .clone()
        .oneshot(Request::get(BACKUP).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let archive = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    let settings_path = format!("/api/decks/{deck}/anki-auto-sync");
    let settings_request = || Request::get(&settings_path).body(Body::empty()).unwrap();
    sqlx::query("UPDATE anki_auto_sync SET profile='Destination profile' WHERE deck_id=$1")
        .bind(deck)
        .execute(&pool)
        .await
        .unwrap();

    let response = app.clone().oneshot(settings_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    let settings: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(settings["profile"], "Destination profile");

    let response = app.clone().oneshot(restore(archive, true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let response = app.oneshot(settings_request()).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 1024).await.unwrap();
    let settings: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        settings,
        serde_json::json!({
            "profile":null,
            "last_synced_at":null,
            "last_received_at":null,
            "last_outcome":null
        })
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM anki_auto_sync")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0,
    );
    assert_eq!(
        sqlx::query_as::<_, (String, i64, i64)>(
            "SELECT profile,received_reviews,sent_reviews FROM anki_sync_history WHERE deck_id=$1",
        )
        .bind(deck)
        .fetch_one(&pool)
        .await
        .unwrap(),
        ("Source profile".into(), 2, 3),
    );
}
