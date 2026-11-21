use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use neondojo::{
    api::{self, AppState},
    engine::Engine,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn request(pool: &PgPool, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
    let app = api::router(AppState::new(pool.clone(), Engine::new("unused".into())));
    let response = app
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
async fn target(pool: &PgPool, label: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf) VALUES($1,$2,'sgf',$3,'Alice','Bob',7,1,'original'::bytea,'(;DT[2025-04-03]EV[Open]RE[W+7])')").bind(id).bind(format!("{label}.sgf")).bind(id.to_string()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO positions(id,match_uuid,ordinal,game_no,ply,player,dice,board,position_id,gnubg_match_id,notation) VALUES($1,$2,0,1,1,0,ARRAY[3,1],array_fill(0,ARRAY[26]),'position','match','8/5')").bind(Uuid::new_v4()).bind(id).execute(pool).await.unwrap();
    id
}
async fn checkpoint_jobs(pool: &PgPool, match_id: Uuid) {
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) SELECT id,coalesce(game_no,1) FROM jobs WHERE match_uuid=$1 AND status IN ('completed','running','failed') ON CONFLICT DO NOTHING")
        .bind(match_id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) SELECT j.id,p.id,coalesce(j.game_no,1),1,p.player,'checker',0.0,'{\"best\":\"8/5 6/5\",\"evaluation_status\":\"evaluated\",\"played_equity\":0,\"best_equity\":0,\"equity_loss\":0}'::jsonb FROM jobs j JOIN LATERAL (SELECT id,player FROM positions WHERE match_uuid=j.match_uuid ORDER BY ordinal LIMIT 1) p ON true WHERE j.match_uuid=$1 AND j.status IN ('completed','running','failed') ON CONFLICT DO NOTHING")
        .bind(match_id).execute(pool).await.unwrap();
}
async fn database(pool: &PgPool, name: &str) -> Uuid {
    let (status, v) = request(
        pool,
        "POST",
        "/api/databases",
        Some(json!({"name":name,"description":"Private"})),
    )
    .await;
    assert_eq!(status, 201, "{v}");
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn default_database_and_optimistic_update(pool: PgPool) {
    let (status, list) = request(&pool, "GET", "/api/databases", None).await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(list[0]["name"], "Unsorted");
    assert_eq!(list[0]["is_default"], true);
    assert_eq!(list[0]["is_system"], true);
    let default_id = list[0]["id"].as_str().unwrap();
    let default_version = list[0]["version"].as_i64().unwrap();
    assert_eq!(
        request(
            &pool,
            "PUT",
            &format!("/api/databases/{default_id}"),
            Some(json!({"name":"Renamed system database","version":default_version}))
        )
        .await
        .0,
        409
    );
    let (status, updated) = request(
        &pool,
        "PUT",
        &format!("/api/databases/{default_id}"),
        Some(json!({"name":"Unsorted","description":"Imports","version":default_version})),
    )
    .await;
    assert_eq!(status, 200, "{updated}");
    assert_eq!(updated["description"], "Imports");
    let default_version = updated["version"].as_i64().unwrap();
    let id = database(&pool, "Tournament").await;
    assert_eq!(
        request(
            &pool,
            "PUT",
            &format!("/api/databases/{id}"),
            Some(json!({"name":"Renamed","description":"","version":1}))
        )
        .await
        .0,
        200
    );
    assert_eq!(
        request(
            &pool,
            "PUT",
            &format!("/api/databases/{id}"),
            Some(json!({"name":"Lost update","description":"","version":1}))
        )
        .await
        .0,
        409
    );
    assert_eq!(
        request(
            &pool,
            "DELETE",
            &format!("/api/databases/{default_id}?version={default_version}&delete_matches=true"),
            None
        )
        .await
        .0,
        409
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn bot_matches_is_provisioned_and_protected_before_play(pool: PgPool) {
    let (_, databases) = request(&pool, "GET", "/api/databases", None).await;
    let bot = databases
        .as_array()
        .unwrap()
        .iter()
        .find(|database| database["name"] == "Bot Matches")
        .unwrap();
    assert_eq!(bot["id"], neondojo::play::BOT_DATABASE_ID.to_string());
    assert_eq!(bot["is_system"], true);
    assert_eq!(bot["is_default"], false);
    let id = bot["id"].as_str().unwrap();
    let version = bot["version"].as_i64().unwrap();
    let path = format!("/api/databases/{id}");
    assert_eq!(
        request(
            &pool,
            "PUT",
            &path,
            Some(json!({"name":"Renamed","version":version}))
        )
        .await
        .0,
        409
    );
    assert_eq!(
        request(
            &pool,
            "DELETE",
            &format!("{path}?version={version}&delete_matches=true"),
            None
        )
        .await
        .0,
        409
    );
    let imported = target(&pool, "Bot fixture").await;
    sqlx::query("UPDATE matches SET database_id=$1 WHERE id=$2")
        .bind(neondojo::play::BOT_DATABASE_ID)
        .bind(imported)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        request(
            &pool,
            "DELETE",
            &format!("{path}/matches?version={version}"),
            None
        )
        .await
        .0,
        204
    );
    let (_, databases) = request(&pool, "GET", "/api/databases", None).await;
    let bot = databases
        .as_array()
        .unwrap()
        .iter()
        .find(|database| database["id"] == id)
        .unwrap();
    assert_eq!(bot["match_count"], 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn clearing_unsorted_preserves_the_system_database_and_other_matches(pool: PgPool) {
    let (_, databases) = request(&pool, "GET", "/api/databases", None).await;
    let system = &databases[0];
    let id = system["id"].as_str().unwrap();
    let version = system["version"].as_i64().unwrap();
    let clear = format!("/api/databases/{id}/matches?version={version}");
    let unsorted = target(&pool, "Unsorted match").await;
    let other = target(&pool, "Sorted match").await;
    let custom = database(&pool, "Tournament").await;
    let (status, moved) = request(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"move","match_ids":[other],"target_database_id":custom})),
    )
    .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(moved["results"][0]["status"], "moved");
    let owner: Uuid = sqlx::query_scalar("SELECT database_id FROM matches WHERE id=$1")
        .bind(unsorted)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owner.to_string(), id);

    let job = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO jobs(id,match_uuid,kind,plies,status) VALUES($1,$2,'match',1,'queued')",
    )
    .bind(job)
    .bind(unsorted)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(request(&pool, "DELETE", &clear, None).await.0, 409);
    assert_eq!(
        request(&pool, "GET", &format!("/api/matches/{unsorted}"), None)
            .await
            .0,
        200
    );

    sqlx::query("DELETE FROM jobs WHERE id=$1")
        .bind(job)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(request(&pool, "DELETE", &clear, None).await.0, 204);
    let (_, after) = request(&pool, "GET", "/api/databases", None).await;
    assert_eq!(after[0]["id"], id);
    assert_eq!(after[0]["name"], "Unsorted");
    assert_eq!(after[0]["is_default"], true);
    assert_eq!(after[0]["match_count"], 0);
    assert!(after[0]["version"].as_i64().unwrap() > version);
    assert_eq!(request(&pool, "DELETE", &clear, None).await.0, 409);
    assert_eq!(
        request(&pool, "GET", &format!("/api/matches/{other}"), None)
            .await
            .0,
        200
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn filters_metadata_and_scope_bound_cursor(pool: PgPool) {
    let a = target(&pool, "A").await;
    let b = target(&pool, "B").await;
    let (status,v)=request(&pool,"PUT",&format!("/api/matches/{a}/metadata"),Some(json!({"version":0,"display_name":"Final","played_on":"2025-04-03","tournament":"Open","round":"Final","place":"Berlin","result_state":"completed","white_score":7,"black_score":3}))).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(
        request(
            &pool,
            "PUT",
            &format!("/api/matches/{a}/metadata"),
            Some(json!({"version":0,"display_name":"Stale","result_state":"unknown"}))
        )
        .await
        .0,
        409
    );
    let (status,v)=request(&pool,"GET","/api/library/matches?player=Ali&date_from=2025-01-01&date_field=played&result=completed&sort=name",None).await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["items"].as_array().unwrap().len(), 1);
    assert_eq!(v["items"][0]["id"], a.to_string());
    assert!(v["items"][0]["white_pr"].is_null());
    let (_, v) = request(&pool, "GET", "/api/library/matches?limit=1&sort=name", None).await;
    let cursor = v["next_cursor"].as_str().unwrap();
    assert_eq!(
        request(
            &pool,
            "GET",
            &format!("/api/library/matches?limit=1&sort=black&after={cursor}"),
            None
        )
        .await
        .0,
        422
    );
    let (_, next) = request(
        &pool,
        "GET",
        &format!("/api/library/matches?limit=1&sort=name&after={cursor}"),
        None,
    )
    .await;
    assert_eq!(next["items"].as_array().unwrap().len(), 1);
    assert_ne!(next["items"][0]["id"], v["items"][0]["id"]);
    assert!(
        [a.to_string(), b.to_string()]
            .contains(&next["items"][0]["id"].as_str().unwrap().to_string())
    );
    assert_eq!(
        request(&pool, "GET", "/api/library/matches?sort=drop_table", None)
            .await
            .0,
        422
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn copy_is_independent_and_move_skips_target_duplicate(pool: PgPool) {
    let id = target(&pool, "Copy").await;
    let db = database(&pool, "Copies").await;
    sqlx::query("INSERT INTO annotations(id,match_uuid,tags,comment,version) VALUES($1,$2,ARRAY['study'],'Original comment',1)").bind(Uuid::new_v4()).bind(id).execute(&pool).await.unwrap();
    let (status, v) = request(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"copy","match_ids":[id],"target_database_id":db})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    assert_eq!(v["results"][0]["status"], "copied");
    let copied = Uuid::parse_str(v["results"][0]["created_id"].as_str().unwrap()).unwrap();
    assert_ne!(id, copied);
    let notes: (String, Vec<String>) =
        sqlx::query_as("SELECT comment,tags FROM annotations WHERE match_uuid=$1")
            .bind(copied)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(notes.0, "Original comment");
    sqlx::query("UPDATE annotations SET comment='Copy only' WHERE match_uuid=$1")
        .bind(copied)
        .execute(&pool)
        .await
        .unwrap();
    let (_, v) = request(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"move","match_ids":[id],"target_database_id":db})),
    )
    .await;
    assert_eq!(v["results"][0]["status"], "duplicate");
    assert_eq!(v["results"][0]["existing_id"], copied.to_string());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM matches")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    let original: String =
        sqlx::query_scalar("SELECT comment FROM annotations WHERE match_uuid=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(original, "Original comment");
    let (status,v)=request(&pool,"POST","/api/library/actions",Some(json!({"action":"tags","match_ids":[copied],"add_tags":[" New  Tag "],"remove_tags":["study"]}))).await;
    assert_eq!(status, 200, "{v}");
    let tags: Vec<String> = sqlx::query_scalar("SELECT tags FROM annotations WHERE match_uuid=$1")
        .bind(copied)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tags, vec!["new tag"]);
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn deletion_guards_active_jobs_and_nonempty_database(pool: PgPool) {
    let id = target(&pool, "Busy").await;
    let db = database(&pool, "Busy database").await;
    request(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"move","match_ids":[id],"target_database_id":db})),
    )
    .await;
    assert_eq!(
        request(
            &pool,
            "DELETE",
            &format!("/api/databases/{db}?version=1"),
            None
        )
        .await
        .0,
        409
    );
    sqlx::query(
        "INSERT INTO jobs(id,match_uuid,kind,plies,status) VALUES($1,$2,'match',1,'running')",
    )
    .bind(Uuid::new_v4())
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let (_, v) = request(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"delete","match_ids":[id]})),
    )
    .await;
    assert_eq!(v["results"][0]["status"], "busy");
    assert_eq!(
        request(
            &pool,
            "DELETE",
            &format!("/api/databases/{db}?version=1&delete_matches=true"),
            None
        )
        .await
        .0,
        409
    );
    assert_eq!(
        request(
            &pool,
            "POST",
            "/api/library/export",
            Some(json!({"match_ids":[id,Uuid::new_v4()],"format":"original"}))
        )
        .await
        .0,
        404
    );
}
#[test]
fn source_metadata_uses_explicit_sgf_context_and_rejects_guesses() {
    let m = neondojo::databases::source_metadata(
        "(;MI[length:7][ws:5][bs:4]DT[2025-04-03]EV[Open]RO[Final]PC[Berlin]RE[W+2R])",
    );
    assert_eq!(m.played_on.unwrap().to_string(), "2025-04-03");
    assert_eq!(m.tournament.as_deref(), Some("Open"));
    assert_eq!(m.result_state, "completed");
    assert_eq!(m.white_score, Some(7));
    assert_eq!(m.black_score, Some(4));
    let m = neondojo::databases::source_metadata("(;DT[2025]RE[W+2])");
    assert!(m.played_on.is_none());
    assert_eq!(m.result_state, "unknown");
    assert!(m.white_score.is_none());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn deep_copy_preserves_all_completed_analyses_and_remaps_selection(pool: PgPool) {
    let id = target(&pool, "Deep").await;
    let db = database(&pool, "Deep copies").await;
    let position: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let game = Uuid::new_v4();
    let mov = Uuid::new_v4();
    sqlx::query("INSERT INTO match_games(id,match_uuid,game_no,position_count,white_score,black_score,crawford) VALUES($1,$2,1,1,0,0,false)").bind(game).bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_moves(id,match_uuid,game_no,move_no,kind,player,notation,board,dice,position_id,gnubg_match_id) VALUES($1,$2,1,1,'cube',0,'Double',array_fill(0,ARRAY[26]),ARRAY[0,0],'position','match')").bind(mov).bind(id).execute(&pool).await.unwrap();
    sqlx::query("UPDATE matches SET moves_cataloged=true WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    for (column, uuid, comment) in [
        ("position_uuid", position, "Position note"),
        ("game_uuid", game, "Game note"),
        ("move_uuid", mov, "Cube note"),
    ] {
        sqlx::query(&format!("INSERT INTO annotations(id,{column},tags,comment,version) VALUES($1,$2,ARRAY['scope'],'{comment}',1)")).bind(Uuid::new_v4()).bind(uuid).execute(&pool).await.unwrap();
    }
    for (n, status, config, rating) in [
        (0, "completed", "old", 1.0),
        (1, "completed", "new", 2.5),
        (2, "running", "running", 9.0),
    ] {
        let result = native_result(rating, 4.0);
        sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,engine_config,result,created_at) VALUES($1,$2,'match',1,$3,1,1,$4,$5,now()+$6*interval '1 second')").bind(Uuid::new_v4()).bind(id).bind(status).bind(config).bind(result).bind(n).execute(&pool).await.unwrap();
    }
    checkpoint_jobs(&pool, id).await;
    sqlx::query("INSERT INTO analysis_selections(match_id,analysis_id) SELECT $1,'job:'||id::text FROM jobs WHERE match_uuid=$1 AND engine_config='old'").bind(id).execute(&pool).await.unwrap();
    let (status, v) = request(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"copy","match_ids":[id],"target_database_id":db})),
    )
    .await;
    assert_eq!(status, 200, "{v}");
    let copied = Uuid::parse_str(v["results"][0]["created_id"].as_str().unwrap()).unwrap();
    let jobs: Vec<(String, String)> = sqlx::query_as(
        "SELECT status,engine_config FROM jobs WHERE match_uuid=$1 ORDER BY engine_config",
    )
    .bind(copied)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        jobs,
        vec![
            ("completed".into(), "new".into()),
            ("completed".into(), "old".into())
        ]
    );
    let terminal:i64=sqlx::query_scalar("SELECT count(*) FROM job_events e JOIN jobs j ON j.id=e.job_id WHERE j.match_uuid=$1 AND e.event='completed'").bind(copied).fetch_one(&pool).await.unwrap();
    assert_eq!(
        terminal, 2,
        "copied completed jobs need a terminal event for resumable SSE"
    );
    let note_targets:Vec<(String,Uuid,String)>=sqlx::query_as("SELECT a.target_kind,a.target_id,a.comment FROM annotations a LEFT JOIN positions p ON p.id=a.position_uuid LEFT JOIN match_games g ON g.id=a.game_uuid LEFT JOIN match_moves m ON m.id=a.move_uuid WHERE coalesce(a.match_uuid,p.match_uuid,g.match_uuid,m.match_uuid)=$1 ORDER BY a.target_kind").bind(copied).fetch_all(&pool).await.unwrap();
    assert_eq!(note_targets.len(), 3);
    for (_, uuid, _) in &note_targets {
        assert!(![position, game, mov].contains(uuid));
    }
    let (_, page) = request(
        &pool,
        "GET",
        &format!("/api/library/matches?database_id={db}&tag=scope&sort=white_pr"),
        None,
    )
    .await;
    assert_eq!(page["items"][0]["white_pr"], 1.0);
    assert_eq!(page["items"][0]["black_pr"], 4.0);
    assert_eq!(page["items"][0]["analysis_status"], "completed");
    let selected_config:String=sqlx::query_scalar("SELECT j.engine_config FROM analysis_selections s JOIN jobs j ON s.analysis_id='job:'||j.id::text WHERE s.match_id=$1").bind(copied).fetch_one(&pool).await.unwrap();
    assert_eq!(selected_config, "old");
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn exports_preserve_bytes_and_reject_missing_or_oversized_selection(pool: PgPool) {
    let a = target(&pool, "One").await;
    let b = target(&pool, "Two").await;
    for (ids, format) in [(vec![a], "original"), (vec![a, b], "sgf")] {
        let app = api::router(AppState::new(pool.clone(), Engine::new("unused".into())));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/library/export")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({"match_ids":ids,"format":format}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        if ids.len() == 1 {
            assert_eq!(&bytes[..], b"original")
        } else {
            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
            assert_eq!(archive.len(), 2);
            for i in 0..2 {
                let mut file = archive.by_index(i).unwrap();
                let mut contents = String::new();
                std::io::Read::read_to_string(&mut file, &mut contents).unwrap();
                assert_eq!(contents, "(;DT[2025-04-03]EV[Open]RE[W+7])");
            }
        }
    }
    assert_eq!(
        request(
            &pool,
            "POST",
            "/api/library/export",
            Some(json!({"match_ids":vec![a;51],"format":"original"}))
        )
        .await
        .0,
        422
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn xg_export_writes_the_played_line_with_edited_library_details(pool: PgPool) {
    let original = include_bytes!("fixtures/WC2014.xg");
    let sgf = neondojo::import::xg_to_sgf(original).unwrap();
    let imported = neondojo::import::parse_sgf(&sgf).unwrap();
    let id = neondojo::store::import_match(&pool, "WC2014.xg", "xg", original, &sgf, &imported)
        .await
        .unwrap()
        .r#match
        .id;
    sqlx::query("UPDATE matches SET tournament='Edited Open',round=NULL WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let app = api::router(AppState::new(pool.clone(), Engine::new("unused".into())));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/library/export")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"match_ids":[id],"format":"xg"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let disposition = response.headers()["content-disposition"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        disposition,
        format!("attachment; filename=\"WC2014-{id}.xg\"")
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let exported = neondojo::import::xg_to_sgf(&bytes).unwrap();
    let metadata = neondojo::databases::source_metadata(&exported);
    assert_eq!(metadata.tournament.as_deref(), Some("Edited Open"));
    assert_eq!(metadata.round, None);
    assert_eq!(metadata.place.as_deref(), Some("Monte-Carlo"));
    let reimported = neondojo::import::parse_sgf(&exported).unwrap();
    assert_eq!(reimported.positions.len(), imported.positions.len());
    for (a, b) in reimported.positions.iter().zip(&imported.positions) {
        assert_eq!((&a.position_id, &a.match_id), (&b.position_id, &b.match_id));
    }
    // A stored source that does not replay is reported, not silently skipped.
    let broken = target(&pool, "Broken").await;
    let (status, error) = request(
        &pool,
        "POST",
        "/api/library/export",
        Some(json!({"match_ids":[id,broken],"format":"xg"})),
    )
    .await;
    assert_eq!(status, 422, "{error}");
    assert!(
        error
            .to_string()
            .contains("Broken.sgf cannot be exported as XG"),
        "{error}"
    );
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn imports_deduplicate_within_each_database(pool: PgPool) {
    let source = include_str!("fixtures/analysis-game1.sgf");
    let imported = neondojo::import::parse_sgf(source).unwrap();
    let db = database(&pool, "Imports").await;
    let one = neondojo::store::import_match(
        &pool,
        "source.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &imported,
    )
    .await
    .unwrap();
    assert!(!one.duplicate);
    let two = neondojo::store::import_match_in_database(
        &pool,
        "source.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &imported,
        db,
    )
    .await
    .unwrap();
    assert!(!two.duplicate);
    assert_ne!(one.r#match.id, two.r#match.id);
    let duplicate = neondojo::store::import_match_in_database(
        &pool,
        "source.sgf",
        "sgf",
        source.as_bytes(),
        source,
        &imported,
        db,
    )
    .await
    .unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.r#match.id, two.r#match.id);
    let (_, page) = request(
        &pool,
        "GET",
        &format!("/api/library/matches?database_id={db}"),
        None,
    )
    .await;
    assert_eq!(page["items"][0]["played_on"], "2014-08-10");
    assert_eq!(page["items"][0]["tournament"], "World Championship 2014");
    assert!(page["items"][0]["white_pr"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn exact_position_occurrences_can_be_scoped_to_a_database(pool: PgPool) {
    let a = target(&pool, "Scoped A").await;
    let b = target(&pool, "Scoped B").await;
    let db = database(&pool, "Scope").await;
    request(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"move","match_ids":[b],"target_database_id":db})),
    )
    .await;
    let scoped = neondojo::store::positions_scoped_in_database(
        &pool,
        None,
        Some("position"),
        None,
        50,
        None,
        Some(db),
    )
    .await
    .unwrap();
    assert_eq!(scoped.items.len(), 1);
    assert_eq!(scoped.items[0].match_uuid, b);
    let all = neondojo::store::positions(&pool, None, Some("position"), None, 50)
        .await
        .unwrap();
    assert_eq!(all.items.len(), 2);
    assert!(all.items.iter().any(|p| p.match_uuid == a));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn malformed_cursor_values_are_validation_errors(pool: PgPool) {
    use base64::Engine as _;
    target(&pool, "Cursor A").await;
    target(&pool, "Cursor B").await;
    let (_, page) = request(
        &pool,
        "GET",
        "/api/library/matches?sort=match_length&limit=1",
        None,
    )
    .await;
    let engine = &base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let mut cursor: Value = serde_json::from_slice(
        &engine
            .decode(page["next_cursor"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    cursor["key"] = json!("invalid integer");
    let cursor = engine.encode(serde_json::to_vec(&cursor).unwrap());
    assert_eq!(
        request(
            &pool,
            "GET",
            &format!("/api/library/matches?sort=match_length&limit=1&after={cursor}"),
            None
        )
        .await
        .0,
        422
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn every_sort_cursor_roundtrips_with_nulls_last(pool: PgPool) {
    let a = target(&pool, "Sort A").await;
    target(&pool, "Sort B").await;
    sqlx::query("UPDATE matches SET played_on='2025-02-01' WHERE id=$1")
        .bind(a)
        .execute(&pool)
        .await
        .unwrap();
    for sort in [
        "created_at",
        "played_at",
        "name",
        "white",
        "black",
        "match_length",
        "result",
        "white_pr",
        "black_pr",
    ] {
        for direction in ["asc", "desc"] {
            let base = format!("/api/library/matches?sort={sort}&direction={direction}&limit=1");
            let (status, first) = request(&pool, "GET", &base, None).await;
            assert_eq!(status, 200, "{base}: {first}");
            let cursor = first["next_cursor"].as_str().unwrap();
            let (status, next) =
                request(&pool, "GET", &format!("{base}&after={cursor}"), None).await;
            assert_eq!(status, 200, "{base}: {next}");
            assert_eq!(next["items"].as_array().unwrap().len(), 1, "{base}: {next}");
            assert_ne!(first["items"][0]["id"], next["items"][0]["id"]);
            let (status, numbered) = request(&pool, "GET", &format!("{base}&page=2"), None).await;
            assert_eq!(status, 200, "{base}: {numbered}");
            assert_eq!(numbered["total"], 2);
            assert_eq!(numbered["items"], next["items"], "{base}");
            if sort == "played_at" {
                assert_eq!(first["items"][0]["id"], a.to_string());
                assert!(next["items"][0]["played_on"].is_null());
            }
        }
    }
}

fn native_result(white: f64, black: f64) -> Value {
    use neondojo::model::{
        MatchAnalysisSummary, PerformanceRatingSummary, PlayerAnalysisSummary,
        PlayerPerformanceRating,
    };
    let result = MatchAnalysisSummary {
        engine: "GNUbg".into(),
        plies: 1,
        total_games: 1,
        completed_games: 1,
        total_moves: 1,
        analyzed_moves: 1,
        players: [
            PlayerAnalysisSummary {
                player: 0,
                ..Default::default()
            },
            PlayerAnalysisSummary {
                player: 1,
                ..Default::default()
            },
        ],
        performance_rating: Some(PerformanceRatingSummary {
            method: "gnubg_emg_500_v1".into(),
            profile: "GNUbg; 1-ply cubeful deterministic".into(),
            games: 1,
            players: [
                PlayerPerformanceRating {
                    player: 0,
                    value: Some(white),
                    normalized_equity_loss: white / 500.0,
                    unforced_checker_moves: 1,
                    close_cube_decisions: 0,
                },
                PlayerPerformanceRating {
                    player: 1,
                    value: Some(black),
                    normalized_equity_loss: black / 500.0,
                    unforced_checker_moves: 1,
                    close_cube_decisions: 0,
                },
            ],
        }),
    };
    let json = serde_json::to_value(result).unwrap();
    assert!(neondojo::match_analysis::match_performance_rating(&json).is_some());
    json
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn latest_completed_missing_native_statistics_suppresses_older_pr(pool: PgPool) {
    let id = target(&pool, "No stale PR").await;
    for (n, result) in [
        (0, native_result(2.5, 4.0)),
        (
            1,
            json!({"total_games":1,"completed_games":1,"performance_rating":null}),
        ),
    ] {
        sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,result,created_at) VALUES($1,$2,'match',1,'completed',1,1,$3,now()+$4*interval '1 second')").bind(Uuid::new_v4()).bind(id).bind(result).bind(n).execute(&pool).await.unwrap();
    }
    checkpoint_jobs(&pool, id).await;
    let (status, page) = request(&pool, "GET", "/api/library/matches", None).await;
    assert_eq!(status, 200, "{page}");
    assert!(page["items"][0]["white_pr"].is_null(), "{page}");
    assert!(page["items"][0]["pr_method"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn position_submission_respects_match_deletion_lock(pool: PgPool) {
    let id = target(&pool, "Deletion race").await;
    let position: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut deletion = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM matches WHERE id=$1 FOR UPDATE")
        .bind(id)
        .execute(&mut *deletion)
        .await
        .unwrap();
    let submit_pool = pool.clone();
    let mut submission = tokio::spawn(async move {
        neondojo::store::create_job(&submit_pool, position, "analysis", 1).await
    });
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(200), &mut submission)
            .await
            .is_err(),
        "position submission must wait for owning-match deletion lock"
    );
    sqlx::query("DELETE FROM matches WHERE id=$1")
        .bind(id)
        .execute(&mut *deletion)
        .await
        .unwrap();
    deletion.commit().await.unwrap();
    let submitted = tokio::time::timeout(std::time::Duration::from_secs(2), submission)
        .await
        .unwrap()
        .unwrap();
    assert!(
        submitted.is_err(),
        "a deleted source cannot accept an analysis job"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn exact_position_cursor_cannot_cross_database_filter(pool: PgPool) {
    let id = target(&pool, "Cursor source").await;
    let db = database(&pool, "Other database").await;
    let position: Uuid = sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let (status, body) = request(
        &pool,
        "GET",
        &format!("/api/positions?match_id={id}&database_id={db}&after={position}"),
        None,
    )
    .await;
    assert_eq!(status, 422, "{body}");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn scoped_game_analysis_has_partial_status_and_never_whole_match_pr(pool: PgPool) {
    let id = target(&pool, "Scoped analysis status").await;
    let scoped = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,game_no,result) VALUES($1,$2,'match',1,'completed',1,1,1,$3)")
        .bind(scoped).bind(id).bind(native_result(2.5,4.0)).execute(&pool).await.unwrap();
    checkpoint_jobs(&pool, id).await;
    let (status, page) = request(&pool, "GET", "/api/library/matches", None).await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(page["items"][0]["analysis_status"], "partial");
    for key in ["white_pr", "black_pr", "pr_method", "pr_profile"] {
        assert!(
            page["items"][0][key].is_null(),
            "scoped statistics must not supply whole-match {key}: {page}"
        );
    }
    let latest = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,game_no,created_at) VALUES($1,$2,'match',1,'queued',1,2,now()+interval '1 second')")
        .bind(latest).bind(id).execute(&pool).await.unwrap();
    for expected in ["queued", "running", "failed"] {
        sqlx::query("UPDATE jobs SET status=$2 WHERE id=$1")
            .bind(latest)
            .bind(expected)
            .execute(&pool)
            .await
            .unwrap();
        let (_, page) = request(&pool, "GET", "/api/library/matches", None).await;
        assert_eq!(
            page["items"][0]["analysis_status"], "partial",
            "new uncompleted work {expected} must not hide saved analysis: {page}"
        );
        assert!(page["items"][0]["white_pr"].is_null());
    }
    // A whole-match status takes precedence even when the scoped job is newer.
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,completed_games,result,created_at) VALUES($1,$2,'match',1,'completed',1,1,$3,now()-interval '1 second')")
        .bind(Uuid::new_v4()).bind(id).bind(native_result(1.0,3.0)).execute(&pool).await.unwrap();
    checkpoint_jobs(&pool, id).await;
    let (_, page) = request(&pool, "GET", "/api/library/matches", None).await;
    assert_eq!(page["items"][0]["analysis_status"], "completed");
    assert_eq!(page["items"][0]["white_pr"], 1.0);
    let rerun = Uuid::new_v4();
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games) VALUES($1,$2,'match',1,'queued',1)")
        .bind(rerun).bind(id).execute(&pool).await.unwrap();
    for (job_status, expected) in [
        ("queued", "queued"),
        ("running", "running"),
        ("failed", "completed"),
    ] {
        sqlx::query("UPDATE jobs SET status=$2 WHERE id=$1")
            .bind(rerun)
            .bind(job_status)
            .execute(&pool)
            .await
            .unwrap();
        let (status, page) = request(&pool, "GET", "/api/library/matches", None).await;
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["items"][0]["analysis_status"], expected, "{page}");
        assert_eq!(page["items"][0]["white_pr"], 1.0, "{page}");
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn actual_result_filters_require_completed_known_scores(pool: PgPool) {
    let white_win = target(&pool, "White wins").await;
    let ongoing = target(&pool, "Ongoing white lead").await;
    let black_win = target(&pool, "Black wins").await;
    let draw = target(&pool, "Draw").await;
    let unknown_score = target(&pool, "Unknown score").await;
    for (id, state, white, black) in [
        (white_win, "completed", Some(7), Some(3)),
        (ongoing, "ongoing", Some(6), Some(1)),
        (black_win, "completed", Some(2), Some(7)),
        (draw, "completed", Some(4), Some(4)),
        (unknown_score, "completed", Some(7), None),
    ] {
        sqlx::query("UPDATE matches SET result_state=$2,white_score=$3,black_score=$4 WHERE id=$1")
            .bind(id)
            .bind(state)
            .bind(white)
            .bind(black)
            .execute(&pool)
            .await
            .unwrap();
    }
    for (filter, expected) in [
        ("white_won", white_win),
        ("black_won", black_win),
        ("draw", draw),
    ] {
        let (status, page) = request(
            &pool,
            "GET",
            &format!("/api/library/matches?result={filter}"),
            None,
        )
        .await;
        assert_eq!(status, 200, "{filter}: {page}");
        assert_eq!(
            page["items"].as_array().unwrap().len(),
            1,
            "{filter}: {page}"
        );
        assert_eq!(page["items"][0]["id"], expected.to_string());
    }
    let (_, completed) = request(&pool, "GET", "/api/library/matches?result=completed", None).await;
    assert_eq!(completed["items"].as_array().unwrap().len(), 4);
    let (_, ongoing_page) =
        request(&pool, "GET", "/api/library/matches?result=ongoing", None).await;
    assert_eq!(ongoing_page["items"][0]["id"], ongoing.to_string());
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn round_filter_is_literal_case_insensitive_and_combines_with_scope(pool: PgPool) {
    let db = database(&pool, "Round search").await;
    let matching = target(&pool, "Round match").await;
    let wrong_round = target(&pool, "Wrong round").await;
    let wrong_tournament = target(&pool, "Wrong tournament").await;
    let other_database = target(&pool, "Other database").await;
    let unknown_round = target(&pool, "Unknown round").await;
    for (id, database_id, tournament, round) in [
        (matching, db, "Autumn Open", Some("Final 100%_A\\B")),
        (wrong_round, db, "Autumn Open", Some("Final 100xxA\\B")),
        (wrong_tournament, db, "Spring Open", Some("Final 100%_A\\B")),
        (
            other_database,
            Uuid::from_u128(1),
            "Autumn Open",
            Some("Final 100%_A\\B"),
        ),
        (unknown_round, db, "Autumn Open", None),
    ] {
        sqlx::query("UPDATE matches SET database_id=$2,tournament=$3,round=$4 WHERE id=$1")
            .bind(id)
            .bind(database_id)
            .bind(tournament)
            .bind(round)
            .execute(&pool)
            .await
            .unwrap();
    }
    for filter in ["%20fInAL%20100%25_A%5CB%20", "100%25_", "A%5CB"] {
        let (status, page) = request(
            &pool,
            "GET",
            &format!("/api/library/matches?database_id={db}&tournament=autumn&round={filter}"),
            None,
        )
        .await;
        assert_eq!(status, 200, "{page}");
        let ids: Vec<_> = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect();
        let expected = if filter == "A%5CB" {
            vec![matching.to_string(), wrong_round.to_string()]
        } else {
            vec![matching.to_string()]
        };
        assert_eq!(ids.len(), expected.len(), "{page}");
        for id in expected {
            assert!(ids.contains(&id.as_str()), "{page}");
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn library_text_search_includes_round_metadata(pool: PgPool) {
    let matching = target(&pool, "Neutral A").await;
    target(&pool, "Neutral B").await;
    sqlx::query("UPDATE matches SET round='Championship 50%_Final' WHERE id=$1")
        .bind(matching)
        .execute(&pool)
        .await
        .unwrap();
    for query in ["cHampionship", "50%25_"] {
        let (status, page) = request(
            &pool,
            "GET",
            &format!("/api/library/matches?q={query}"),
            None,
        )
        .await;
        assert_eq!(status, 200, "{page}");
        assert_eq!(page["items"].as_array().unwrap().len(), 1, "{page}");
        assert_eq!(page["items"][0]["id"], matching.to_string());
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn library_round_filter_is_bound_to_cursor_scope(pool: PgPool) {
    for name in ["First", "Second"] {
        let id = target(&pool, name).await;
        sqlx::query("UPDATE matches SET round='Final' WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let base = "/api/library/matches?sort=name&limit=1&round=Final";
    let (status, first) = request(&pool, "GET", base, None).await;
    assert_eq!(status, 200, "{first}");
    let cursor = first["next_cursor"].as_str().unwrap();
    let (status, next) = request(&pool, "GET", &format!("{base}&after={cursor}"), None).await;
    assert_eq!(status, 200, "{next}");
    assert_eq!(next["items"].as_array().unwrap().len(), 1);
    assert_ne!(first["items"][0]["id"], next["items"][0]["id"]);
    for round in ["Other", ""] {
        let (status, page) = request(
            &pool,
            "GET",
            &format!("/api/library/matches?sort=name&limit=1&round={round}&after={cursor}"),
            None,
        )
        .await;
        assert_eq!(status, 422, "{page}");
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn library_round_filter_rejects_overlong_text(pool: PgPool) {
    let (status, body) = request(
        &pool,
        "GET",
        &format!("/api/library/matches?round={}", "x".repeat(241)),
        None,
    )
    .await;
    assert_eq!(status, 422, "{body}");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn source_metadata_backfill_recovers_original_results_and_preserves_manual_edits(
    pool: PgPool,
) {
    let bytes = include_bytes!("fixtures/WC2014.xg");
    let converted = neondojo::import::xg_to_sgf(bytes).unwrap();
    let old_canonical = regex::Regex::new(r"RE\[[^\]]*\]")
        .unwrap()
        .replace_all(&converted, "")
        .into_owned();
    let mut ids = Vec::new();
    for manual in [false, true] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,original,sgf,result_state,white_score,black_score,metadata_version,display_name,source_metadata_loaded) VALUES($1,'source.xg','xg',$2,'White','Black',11,460,$3,$4,'ongoing',9,9,$5,$6,true)")
            .bind(id).bind(id.to_string()).bind(bytes.as_slice()).bind(&old_canonical).bind(i64::from(manual)).bind(if manual { "My tournament final" } else { "" }).execute(&pool).await.unwrap();
        ids.push(id);
    }
    // Requeue source-derived metadata as the result repair did: every import
    // was already source-loaded, while manual saves own their fields.
    let changed = sqlx::query(
        "UPDATE matches SET source_metadata_loaded=false
         WHERE metadata_version=0 AND format IN ('bgf','xg')",
    )
    .execute(&pool)
    .await
    .unwrap()
    .rows_affected();
    assert_eq!(changed, 1);
    assert_eq!(
        neondojo::databases::backfill_source_metadata(&pool)
            .await
            .unwrap(),
        1
    );
    let recovered: (String, Option<i32>, Option<i32>, String, bool, i64) = sqlx::query_as("SELECT result_state,white_score,black_score,sgf,source_metadata_loaded,metadata_version FROM matches WHERE id=$1")
        .bind(ids[0]).fetch_one(&pool).await.unwrap();
    assert_eq!(
        (recovered.0.as_str(), recovered.1, recovered.2),
        ("completed", Some(11), Some(9))
    );
    assert_eq!(
        recovered.3, old_canonical,
        "repair must leave imported SGF and position context immutable"
    );
    assert!(recovered.4);
    assert_eq!(recovered.5, 0);
    let manual: (String, Option<i32>, Option<i32>, String, i64, bool) = sqlx::query_as("SELECT result_state,white_score,black_score,display_name,metadata_version,source_metadata_loaded FROM matches WHERE id=$1")
        .bind(ids[1]).fetch_one(&pool).await.unwrap();
    assert_eq!(
        manual,
        (
            "ongoing".to_owned(),
            Some(9),
            Some(9),
            "My tournament final".to_owned(),
            1,
            true
        )
    );
    assert_eq!(
        neondojo::databases::backfill_source_metadata(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn native_context_backfill_fills_gaps_and_preserves_manual_saves(pool: PgPool) {
    let bytes = include_bytes!("fixtures/WC2014.xg");
    let canonical = neondojo::import::xg_to_sgf(bytes).unwrap();
    let legacy = regex::Regex::new(r"(?:DT|EV|RO|PC)\[[^\]]*\]")
        .unwrap()
        .replace_all(&canonical, "");
    let mut ids = Vec::new();
    for (version, source) in [
        (0, bytes.as_slice()),
        (1, bytes.as_slice()),
        (0, b"damaged"),
    ] {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO matches(id,filename,format,sha256,white,black,match_length,position_count,
             original,sgf,tournament,result_state,white_score,black_score,metadata_version,
             source_metadata_loaded)
             VALUES($1,'source.xg','xg',$2,'White','Black',11,460,$3,$4,'Saved event',
             'ongoing',9,9,$5,true)",
        )
        .bind(id)
        .bind(id.to_string())
        .bind(source)
        .bind(legacy.as_ref())
        .bind(version)
        .execute(&pool)
        .await
        .unwrap();
        ids.push(id);
    }

    // Queue imports with missing native context, as an older library has them.
    // A legacy manual save owns the whole metadata form, including empty fields.
    sqlx::query(
        "UPDATE matches SET native_match_context_pending=true
         WHERE format IN ('xg','bgf') AND NOT collection_only AND metadata_version=0
           AND (played_on IS NULL OR tournament IS NULL OR round IS NULL OR place IS NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        neondojo::databases::backfill_source_metadata(&pool)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        neondojo::databases::backfill_source_metadata(&pool)
            .await
            .unwrap(),
        0
    );
    for (index, id) in ids.into_iter().enumerate() {
        let match_row: Value = sqlx::query_scalar("SELECT to_jsonb(m) FROM matches m WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(match_row["sgf"], legacy.as_ref());
        assert_eq!(match_row["tournament"], "Saved event");
        assert_eq!(match_row["result_state"], "ongoing");
        assert_eq!(match_row["white_score"], 9);
        assert_eq!(match_row["black_score"], 9);
        assert_eq!(match_row["native_match_context_pending"], false);
        if index == 0 {
            assert_eq!(match_row["played_on"], "2014-08-10");
            assert_eq!(match_row["round"], "Final - Tie Breaker");
            assert_eq!(match_row["place"], "Monte-Carlo");
            assert_eq!(match_row["metadata_version"], 1);
        } else {
            for field in ["played_on", "round", "place"] {
                assert!(match_row[field].is_null());
            }
            assert_eq!(match_row["metadata_version"], i64::from(index == 1));
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn bgf_import_and_backfill_agree_on_match_context(pool: PgPool) {
    let (header, payload) = include_str!("fixtures/tutorial.bgf")
        .split_once('\n')
        .unwrap();
    let mut payload: Value = serde_json::from_str(payload).unwrap();
    payload["date"] = json!("2015.11.24");
    payload["event"] = json!("Super Jackpot 2015");
    payload["round"] = json!("Final");
    payload["location"] = json!("Berlin");
    let source = format!("{header}\n{payload}");
    let mut upload = b"--bg\r\nContent-Disposition: form-data; name=\"file\"; filename=\"match.bgf\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();
    upload.extend_from_slice(source.as_bytes());
    upload.extend_from_slice(b"\r\n--bg--\r\n");
    let response = api::router(AppState::new(pool.clone(), Engine::new("unused".into())))
        .oneshot(
            Request::post("/api/imports")
                .header("content-type", "multipart/form-data; boundary=bg")
                .body(Body::from(upload))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(status, 201, "{}", String::from_utf8_lossy(&body));
    let imported: Value = serde_json::from_slice(&body).unwrap();
    let id = Uuid::parse_str(imported["match"]["id"].as_str().unwrap()).unwrap();
    let path = format!("/api/matches/{id}/metadata");
    let (status, initial) = request(&pool, "GET", &path, None).await;
    assert_eq!(status, 200, "{initial}");

    sqlx::query(
        "UPDATE matches SET played_on=NULL,tournament=NULL,round=NULL,place=NULL,
                            native_match_context_pending=true WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        neondojo::databases::backfill_source_metadata(&pool)
            .await
            .unwrap(),
        1
    );
    let (status, recovered) = request(&pool, "GET", &path, None).await;
    assert_eq!(status, 200, "{recovered}");
    for (key, value) in [
        ("played_on", "2015-11-24"),
        ("tournament", "Super Jackpot 2015"),
        ("round", "Final"),
        ("place", "Berlin"),
    ] {
        assert_eq!(initial[key], value);
        assert_eq!(recovered[key], value);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn new_native_import_stores_final_result_in_the_library(pool: PgPool) {
    let bytes = include_bytes!("fixtures/WC2014.xg");
    let sgf = neondojo::import::xg_to_sgf(bytes).unwrap();
    let imported = neondojo::import::parse_sgf(&sgf).unwrap();
    let result = neondojo::store::import_match(&pool, "sample.xg", "xg", bytes, &sgf, &imported)
        .await
        .unwrap();
    let (status, page) = request(&pool, "GET", "/api/library/matches", None).await;
    assert_eq!(status, 200);
    assert_eq!(page["items"][0]["id"], result.r#match.id.to_string());
    assert_eq!(page["items"][0]["played_on"], "2014-08-10");
    assert_eq!(page["items"][0]["tournament"], "World Championship 2014");
    assert_eq!(page["items"][0]["round"], "Final - Tie Breaker");
    assert_eq!(page["items"][0]["place"], "Monte-Carlo");
    assert_eq!(page["items"][0]["white_score"], 11);
    assert_eq!(page["items"][0]["black_score"], 9);
    assert_eq!(page["items"][0]["result_state"], "completed");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL"]
async fn copying_a_stale_import_selection_does_not_promote_its_revision(pool: PgPool) {
    let source = target(&pool, "Stale selection").await;
    let destination = database(&pool, "Stale selection copy").await;
    sqlx::query("UPDATE matches SET source_revision=1,moves_cataloged=true,imported_analysis_loaded=true WHERE id=$1").bind(source).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO analysis_selections(match_id,analysis_id,source_revision) VALUES($1,$2,0)",
    )
    .bind(source)
    .bind(format!("imported:{source}"))
    .execute(&pool)
    .await
    .unwrap();
    let (status, response) = request(
        &pool,
        "POST",
        "/api/library/actions",
        Some(json!({"action":"copy","match_ids":[source],"target_database_id":destination})),
    )
    .await;
    assert_eq!(status, 200, "{response}");
    let copied = Uuid::parse_str(response["results"][0]["created_id"].as_str().unwrap()).unwrap();
    let selected: (String, i64) = sqlx::query_as(
        "SELECT analysis_id,source_revision FROM analysis_selections WHERE match_id=$1",
    )
    .bind(copied)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        selected,
        (format!("imported:{copied}"), 0),
        "a stale selection must remain stale on the copied source"
    );
}
