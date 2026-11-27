use axum::{
    body::{Body, Bytes, to_bytes},
    http::{Request, StatusCode, header},
};
use neondojo::{
    api::{AppState, router},
    engine::Engine,
};
use sqlx::PgPool;
use tower::ServiceExt;

const BACKUP: &str = "/api/database-backup";
const CHECK: &str = "/api/database-backup/check";
const MEDIA_TYPE: &str = "application/vnd.neondojo.backup";

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

async fn migrate_through(pool: &PgPool, version: i64) {
    let mut migrations = sqlx::migrate!("./migrations");
    migrations
        .migrations
        .to_mut()
        .retain(|m| m.version <= version);
    migrations.run(pool).await.unwrap();
}

async fn export_archive(app: &axum::Router) -> Bytes {
    let response = app
        .clone()
        .oneshot(Request::get(BACKUP).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap()
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

async fn analysis_snapshot(pool: &PgPool, match_id: uuid::Uuid) -> serde_json::Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
             'source_revision',m.source_revision,
             'effective_selection',(SELECT to_jsonb(e) FROM effective_analysis_selections e WHERE e.match_id=m.id),
             'selection',(SELECT to_jsonb(s) FROM analysis_selections s WHERE s.match_id=m.id),
             'jobs',(SELECT jsonb_agg(to_jsonb(j) ORDER BY j.id) FROM jobs j
                 WHERE j.match_uuid=m.id AND j.status='completed'),
             'checkpoints',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.job_id,c.game_no)
                 FROM match_analysis_games c JOIN jobs j ON j.id=c.job_id WHERE j.match_uuid=m.id),
             'decisions',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id)
                 FROM move_analyses a JOIN jobs j ON j.id=a.job_id WHERE j.match_uuid=m.id),
             'imports',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id)
                 FROM imported_analyses a WHERE a.match_uuid=m.id),
             'projections',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.imported_analysis_id)
                 FROM imported_analysis_projections p JOIN imported_analyses a ON a.id=p.imported_analysis_id
                 WHERE a.match_uuid=m.id))
         FROM matches m WHERE m.id=$1",
    )
    .bind(match_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = false)]
async fn restores_pre_cube_rule_archive_using_each_games_sgf(pool: PgPool) {
    // Export real pre-0095 COPY rows and their original manifest fingerprint.
    migrate_through(&pool, 94).await;
    let (id, _) = insert_match(&pool, "mixed-rules.sgf").await;
    let sgf = r"(;GM[6]MI[length:0]RU[No\Cube];W[31hefe])(;GM[6]MI[length:0];W[31hefe])";
    sqlx::query("UPDATE matches SET sgf=$2 WHERE id=$1")
        .bind(id)
        .bind(sgf)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO match_games(id,match_uuid,game_no,position_count,white_score,black_score,crawford)
        SELECT gen_random_uuid(),$1,n,1,0,0,false FROM generate_series(1,2) n")
        .bind(id).execute(&pool).await.unwrap();
    let app = router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    let archive = export_archive(&app).await;
    neondojo::schema_migrations::MIGRATOR
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM match_games WHERE match_uuid=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        check_result(&app, archive.clone()).await.1["status"],
        "compatible"
    );
    let response = app
        .clone()
        .oneshot(restore(archive.clone(), true))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let rules: Vec<bool> = sqlx::query_scalar(
        "SELECT cube_enabled FROM match_games WHERE match_uuid=$1 ORDER BY game_no",
    )
    .bind(id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rules, vec![false, true]);

    sqlx::query(
        "INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time)
        SELECT $1,'unrelated layout change',true,checksum,0 FROM _sqlx_migrations WHERE version=95",
    )
    .bind(i64::MAX)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        check_result(&app, archive).await.1["status"],
        "incompatible_schema"
    );
}

#[sqlx::test(migrations = false)]
async fn restores_selected_import_from_before_selection_indexes(pool: PgPool) {
    migrate_through(&pool, 95).await;
    let (match_id, _) = crate::support::foreign_analysis(&pool).await;
    let selected = format!("imported:{match_id}");
    sqlx::query(
        "INSERT INTO analysis_selections(match_id, analysis_id, source_revision)
         SELECT id, $2, source_revision FROM matches WHERE id=$1",
    )
    .bind(match_id)
    .bind(&selected)
    .execute(&pool)
    .await
    .unwrap();
    let before = analysis_snapshot(&pool, match_id).await;
    assert_eq!(before["effective_selection"]["analysis_id"], selected);
    assert_eq!(before["imports"].as_array().unwrap().len(), 2);
    let app = router(AppState::new(pool.clone(), Engine::new("unused".into())));
    let archive = export_archive(&app).await;

    neondojo::schema_migrations::MIGRATOR
        .run(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        check_result(&app, archive.clone()).await.1["status"],
        "compatible"
    );
    let response = app.oneshot(restore(archive, true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(analysis_snapshot(&pool, match_id).await, before);
    let source: Vec<u8> = sqlx::query_scalar("SELECT original FROM matches WHERE id=$1")
        .bind(match_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(source, crate::support::FOREIGN_SOURCE.as_bytes());
}

#[sqlx::test(migrations = false)]
async fn restores_and_repairs_legacy_neongammon_labels(pool: PgPool) {
    use serde_json::json;
    migrate_through(&pool, 96).await;
    let sgf = "(;GM[6]RU[NoCube];W[31hefe])";
    let imported = neondojo::import::parse_sgf(sgf).unwrap();
    let id = neondojo::store::import_match(
        &pool,
        "legacy-duel.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &imported,
    )
    .await
    .unwrap()
    .r#match
    .id;
    let legacy = json!({
        "engine": "NeonGammon", "method": "evaluation", "depth": "2-ply cubeful", "depth_plies": 2,
        "raw": {"version": 1, "native_id": "4HPwATDgc/ABMA:MIEFAAAAAAAE"},
        "candidates": [{"equity": 0.125, "equity_kind": "cubeful", "depth": "2-ply cubeful"}]
    });
    let analysis: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO imported_analyses(match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data)
         SELECT match_uuid,id,0,game_no,1,'checker',$2 FROM positions WHERE match_uuid=$1 RETURNING imported_analyses.id")
        .bind(id).bind(&legacy).fetch_one(&pool).await.unwrap();
    let app = router(AppState::new(pool.clone(), Engine::new("unused".into())));
    let archive = export_archive(&app).await;
    neondojo::schema_migrations::MIGRATOR
        .run(&pool)
        .await
        .unwrap();
    let response = app.oneshot(restore(archive, true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let data: serde_json::Value =
        sqlx::query_scalar("SELECT data FROM imported_analyses WHERE id=$1")
            .bind(analysis)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut expected = legacy;
    expected["depth"] = json!("2-ply cubeless");
    expected["candidates"][0]["depth"] = json!("2-ply cubeless");
    expected["candidates"][0]["equity_kind"] = json!("cubeless");
    assert_eq!(data, expected);
}

#[sqlx::test(migrations = "./migrations")]
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
    sqlx::query("UPDATE matches SET source_revision=7 WHERE id=$1")
        .bind(source_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO match_games(id,match_uuid,game_no,position_count,white_score,black_score,crawford) VALUES(gen_random_uuid(),$1,1,1,0,0,false)")
        .bind(source_id).execute(&pool).await.unwrap();
    let selected_job = uuid::Uuid::new_v4();
    for (job, depth, loss) in [(selected_job, 1, 0.125), (uuid::Uuid::new_v4(), 3, 0.25)] {
        sqlx::query(
            "INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,result)
             VALUES($1,$2,'match',$3,'completed',1,1,$4)",
        )
        .bind(job)
        .bind(source_id)
        .bind(depth)
        .bind(serde_json::json!({"engine":"GNU Backgammon", "total_games":1,"completed_games":1}))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,1,$2)")
            .bind(job)
            .bind(serde_json::json!({"total_games":1,"completed_games":1}))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data)
             VALUES($1,$2,1,1,0,'checker',$3,$4)",
        )
        .bind(job)
        .bind(position)
        .bind(loss)
        .bind(serde_json::json!({"kind":"checker","played":"13/10 6/5","equity_loss":loss}))
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO analysis_selections(match_id,analysis_id,source_revision) VALUES($1,$2,7)",
    )
    .bind(source_id)
    .bind(format!("job:{selected_job}"))
    .execute(&pool)
    .await
    .unwrap();
    for (ordinal, kind, projection) in [
        (
            0,
            "checker",
            Some(serde_json::json!({"equity_loss":0.0625,"player":0})),
        ),
        (1, "cube", None),
    ] {
        let imported_id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO imported_analyses(id,match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data)
             VALUES($1,$2,$3,$4,1,1,$5,$6)",
        )
        .bind(imported_id)
        .bind(source_id)
        .bind(position)
        .bind(ordinal)
        .bind(kind)
        .bind(serde_json::json!({"kind":kind,"engine":"Source engine","raw":{"equity_loss":0.0625,"original":"0.062500"}}))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO imported_analysis_projections(imported_analysis_id,revision,source_revision,projection) VALUES($1,3,7,$2)")
            .bind(imported_id).bind(projection).execute(&pool).await.unwrap();
    }
    let saved_analyses = analysis_snapshot(&pool, source_id).await;
    assert_eq!(saved_analyses["source_revision"], 7);
    assert_eq!(saved_analyses["selection"]["source_revision"], 7);
    for table in ["jobs", "checkpoints", "decisions", "imports", "projections"] {
        assert_eq!(
            saved_analyses[table].as_array().unwrap().len(),
            2,
            "{table}"
        );
    }
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
    let target_counts = app
        .clone()
        .oneshot(
            Request::get("/api/database-backup/target-counts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(target_counts.status(), StatusCode::OK);
    let body = to_bytes(target_counts.into_body(), 16 * 1024)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"matches":1,"saved_positions":1,"learning_cards":2})
    );
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
            .contains(".neondojo-backup")
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
            "Invalid NeonDojo backup archive",
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
        analysis_snapshot(&pool, source_id).await,
        saved_analyses,
        "restore must preserve the selected revision, native history, original imports and derived projections exactly"
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

/// Triggers a restore disables while it copies the table they belong to.
const RESTORE_SUSPENDED_TRIGGERS: &[&str] = &[
    "catalog_match_players",
    "positions_played_record_insert",
    "imported_analyses_revision_insert",
    "job_events_notify",
    "match_analysis_games_revision_insert",
    "move_analyses_revision_insert",
];

async fn suspended_trigger_states(pool: &PgPool) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT t.tgname::text, t.tgenabled::text FROM pg_trigger t
         JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace
         WHERE n.nspname='public' AND t.tgname::text=ANY($1) ORDER BY t.tgname",
    )
    .bind(RESTORE_SUSPENDED_TRIGGERS)
    .fetch_all(pool)
    .await
    .unwrap()
}

fn assert_triggers_armed(states: &[(String, String)]) {
    assert_eq!(states.len(), RESTORE_SUSPENDED_TRIGGERS.len(), "{states:?}");
    assert!(
        states.iter().all(|(_, enabled)| enabled == "O"),
        "every suspended trigger is enabled again: {states:?}"
    );
}

#[derive(Debug, PartialEq)]
struct DerivedState {
    fingerprints: Vec<(uuid::Uuid, Option<Vec<u8>>)>,
    native_revisions: Vec<uuid::Uuid>,
    imported_revisions: Vec<uuid::Uuid>,
}

async fn derived_state(pool: &PgPool) -> DerivedState {
    DerivedState {
        fingerprints: sqlx::query_as(
            "SELECT id,played_record_fingerprint FROM matches WHERE NOT collection_only ORDER BY id",
        )
        .fetch_all(pool)
        .await
        .unwrap(),
        native_revisions: sqlx::query_scalar(
            "SELECT job_id FROM native_analysis_revisions ORDER BY job_id",
        )
        .fetch_all(pool)
        .await
        .unwrap(),
        imported_revisions: sqlx::query_scalar(
            "SELECT match_id FROM imported_analysis_revisions ORDER BY match_id",
        )
        .fetch_all(pool)
        .await
        .unwrap(),
    }
}

async fn revision_tokens(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT revision FROM native_analysis_revisions
         UNION SELECT revision FROM imported_analysis_revisions ORDER BY 1",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn drain_job_notifications(listener: &mut sqlx::postgres::PgListener) -> Vec<String> {
    let mut payloads = Vec::new();
    while let Ok(notification) =
        tokio::time::timeout(std::time::Duration::from_millis(500), listener.recv()).await
    {
        payloads.push(notification.unwrap().payload().to_owned());
    }
    payloads.sort();
    payloads
}

#[sqlx::test(migrations = "./migrations")]
async fn restore_copies_history_without_derived_triggers_or_notifications(pool: PgPool) {
    let app = router(AppState::new(pool.clone(), Engine::new("gnubg".into())));
    let mut running = None;
    for index in 0..3_i32 {
        let (match_id, _) = insert_match(&pool, &format!("library-{index}.sgf")).await;
        sqlx::query("INSERT INTO match_games(match_uuid,game_no,position_count,white_score,black_score,crawford) VALUES($1,1,2,0,0,false)")
            .bind(match_id).execute(&pool).await.unwrap();
        let mut positions = Vec::new();
        for ordinal in 1..=2_i32 {
            positions.push(sqlx::query_scalar::<_, uuid::Uuid>(
                "INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation)
                 VALUES(gen_random_uuid(),$1,$2,1,$2,$2 % 2,ARRAY[$2 + 2,$3 + 1],array_fill(0,ARRAY[26]),$4,$4,'13/10 6/5')
                 RETURNING id",
            )
            .bind(match_id)
            .bind(ordinal)
            .bind(index)
            .bind(format!("position-{index}-{ordinal}"))
            .fetch_one(&pool)
            .await
            .unwrap());
        }
        // Two analysed matches; the third has a running checkpointed job and a
        // failed job without results.
        let jobs: &[(&str, bool)] = if index < 2 {
            &[("completed", true)]
        } else {
            &[("running", false), ("failed", false)]
        };
        for &(status, decisions) in jobs {
            let job = uuid::Uuid::new_v4();
            sqlx::query(
                "INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,lease_token,lease_until)
                 VALUES($1,$2,'match',2,$3,1,$4,
                     CASE WHEN $3='running' THEN gen_random_uuid() END,
                     CASE WHEN $3='running' THEN now()+interval '1 hour' END)",
            )
            .bind(job)
            .bind(match_id)
            .bind(status)
            .bind(i32::from(status == "completed"))
            .execute(&pool)
            .await
            .unwrap();
            for event in ["queued", status] {
                sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,$2,'{}')")
                    .bind(job)
                    .bind(event)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
            if status == "failed" {
                continue;
            }
            sqlx::query(
                "INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,1,'{}')",
            )
            .bind(job)
            .execute(&pool)
            .await
            .unwrap();
            if status == "running" {
                running = Some(job);
            }
            if decisions {
                for (decision, position) in (1_i32..).zip(&positions) {
                    sqlx::query(
                        "INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data)
                         VALUES($1,$2,1,$3,0,'checker',0.1,'{}')",
                    )
                    .bind(job)
                    .bind(position)
                    .bind(decision)
                    .execute(&pool)
                    .await
                    .unwrap();
                }
            }
        }
        if index != 1 {
            sqlx::query(
                "INSERT INTO imported_analyses(id,match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data)
                 VALUES(gen_random_uuid(),$1,$2,0,1,1,'checker','{}')",
            )
            .bind(match_id)
            .bind(positions[0])
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    let running = running.unwrap();
    // The rows the triggers produce while the library is written normally.
    let source = derived_state(&pool).await;
    assert_eq!(source.fingerprints.len(), 3);
    assert!(source.fingerprints.iter().all(|(_, value)| value.is_some()));
    assert_eq!(source.native_revisions.len(), 3);
    assert_eq!(source.imported_revisions.len(), 2);
    let source_tokens = revision_tokens(&pool).await;

    let response = app
        .clone()
        .oneshot(Request::get(BACKUP).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let archive = to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    sqlx::query("DELETE FROM matches")
        .execute(&pool)
        .await
        .unwrap();

    let mut listener = sqlx::postgres::PgListener::connect_with(&pool)
        .await
        .unwrap();
    listener.listen("neondojo_job_events").await.unwrap();
    let response = app
        .clone()
        .oneshot(restore(archive.clone(), true))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    // Historical events are not announced; only the job the restore
    // interrupts receives a new event.
    assert_eq!(
        drain_job_notifications(&mut listener).await,
        vec![running.to_string()]
    );
    assert_triggers_armed(&suspended_trigger_states(&pool).await);
    let restored = derived_state(&pool).await;
    assert_eq!(restored, source);
    assert_eq!(
        sqlx::query_as::<_, (uuid::Uuid, Option<Vec<u8>>)>(
            "SELECT match_uuid,fingerprint
             FROM played_record_fingerprints(ARRAY(SELECT id FROM matches)) ORDER BY match_uuid",
        )
        .fetch_all(&pool)
        .await
        .unwrap(),
        source.fingerprints,
    );
    // Every revision carries the restore transaction's token, as the triggers
    // write it.
    let tokens = revision_tokens(&pool).await;
    assert_eq!(tokens.len(), 1, "{tokens:?}");
    assert!(!source_tokens.contains(&tokens[0]));
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT bool_and(substr($1,4)::xid8::xid = m.xmin) FROM matches m",
        )
        .bind(&tokens[0])
        .fetch_one(&pool)
        .await
        .unwrap()
    );

    // A restore that fails after every table was copied leaves the triggers
    // armed and the target unchanged.
    let invalid = rewrite_entry(
        &archive,
        "tables/engine_match_results.copy",
        b"invalid uuid\n",
    );
    let response = app.oneshot(restore(invalid, true)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        drain_job_notifications(&mut listener).await,
        Vec::<String>::new()
    );
    assert_triggers_armed(&suspended_trigger_states(&pool).await);
    assert_eq!(derived_state(&pool).await, source);
    assert_eq!(revision_tokens(&pool).await, tokens);
}
