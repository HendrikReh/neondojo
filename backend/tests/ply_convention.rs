//! Migration 0079 moves every stored GNUbg depth to NeonDojo's single ply
//! convention (XG / Open Sage: 1-ply is the raw evaluation) and leaves Open
//! Sage, XG and source text unchanged.
use neondojo::{import, store};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const CONVENTION_MIGRATION: i64 = 79;

async fn migrate_before(pool: &PgPool) {
    let all = sqlx::migrate!("./migrations");
    let previous = sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.iter()
                .filter(|m| m.version < CONVENTION_MIGRATION)
                .cloned()
                .collect(),
        ),
        ..sqlx::migrate::Migrator::DEFAULT
    };
    previous.run(pool).await.unwrap();
}

async fn json_of(pool: &PgPool, sql: &str, id: Uuid) -> Value {
    sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = false)]
#[ignore = "requires PostgreSQL"]
async fn gnubg_depths_move_to_the_shared_convention(pool: PgPool) {
    migrate_before(&pool).await;
    sqlx::query("INSERT INTO app_settings(singleton,gnubg_path,analysis_engine,analysis_plies,play_engine,play_plies) VALUES(true,'gnubg','gnubg',2,'gnubg',0) ON CONFLICT(singleton) DO UPDATE SET analysis_engine='gnubg',analysis_plies=2,play_engine='gnubg',play_plies=0")
        .execute(&pool).await.unwrap();

    let sgf = include_str!("fixtures/analysis-game1.sgf");
    let parsed = import::parse_sgf(sgf).unwrap();
    let imported = store::import_match(&pool, "ply.sgf", "sgf", sgf.as_bytes(), sgf, &parsed)
        .await
        .unwrap();
    let match_id = imported.r#match.id;
    let position_id: Uuid =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 ORDER BY ordinal LIMIT 1")
            .bind(match_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    // Records as the importer stored them before 0079: GNUbg's native level.
    sqlx::query("DELETE FROM imported_analyses WHERE match_uuid=$1")
        .bind(match_id)
        .execute(&pool)
        .await
        .unwrap();
    let gnubg_import = Uuid::new_v4();
    let xg_import = Uuid::new_v4();
    let gnubg_record = json!({
        "engine": "GNU Backgammon", "kind": "checker", "method": "evaluation",
        "depth": "0-ply cubeful", "depth_plies": 0,
        "candidates": [
            {"rank": 1, "notation": "8/5 6/5", "depth": "2-ply cubeful", "depth_plies": 2,
             "raw": {"value": "lqae E ver 3 0.5 0.1 0.0 0.1 0.0 0.0 2C 0 1 0.000000 1"}},
            {"rank": 2, "notation": "24/23 13/10", "depth": "0-ply cubeless", "depth_plies": 0}
        ],
        "raw": {"A": ["", "lqae E ver 3"], "AP": "GNU Backgammon:1.08"}
    });
    let xg_record = json!({
        "engine": "eXtreme Gammon", "kind": "checker", "method": "evaluation",
        "depth": "3-ply", "depth_plies": 3,
        "candidates": [{"rank": 1, "notation": "8/5 6/5", "depth": "3-ply", "depth_plies": 3}],
        "raw": {"record_index": 1}
    });
    for (id, ordinal, data) in [(gnubg_import, 1, &gnubg_record), (xg_import, 2, &xg_record)] {
        sqlx::query("INSERT INTO imported_analyses(id,match_uuid,position_uuid,source_ordinal,game_no,source_move_no,kind,data) VALUES($1,$2,$3,$4,1,1,'checker',$5)")
            .bind(id).bind(match_id).bind(position_id).bind(ordinal).bind(data)
            .execute(&pool).await.unwrap();
    }

    let gnubg_snapshot = json!({"engine_id": "gnubg", "gnubg_path": "gnubg"});
    let sage_snapshot = json!({"engine_id": "bgsage", "gnubg_path": "gnubg"});
    let candidate = |plies: i32| json!({"rank": 1, "notation": "8/5 6/5", "equity": 0.1, "probabilities": [0.5, 0.1, 0.0, 0.5, 0.1, 0.0], "plies": plies});
    let analysis_job = Uuid::new_v4();
    let dice_job = Uuid::new_v4();
    let legacy_job = Uuid::new_v4();
    let sage_job = Uuid::new_v4();
    for (id, kind, plies, snapshot, result) in [
        (
            analysis_job,
            "analysis",
            2,
            Some(&gnubg_snapshot),
            json!({"engine": "GNU Backgammon 1.08", "plies": 2, "candidates": [candidate(2), candidate(0)]}),
        ),
        (
            dice_job,
            "dice",
            0,
            Some(&gnubg_snapshot),
            json!({"engine": "GNU Backgammon 1.08", "plies": 0, "mean_equity": 0.0,
                   "rolls": [{"dice": [1, 1], "weight": 1, "probability": 0.0277, "best": candidate(0)}]}),
        ),
        (
            legacy_job,
            "analysis",
            1,
            None,
            json!({"engine": "GNU Backgammon 1.08", "plies": 1, "candidates": [candidate(1)]}),
        ),
        (
            sage_job,
            "analysis",
            2,
            Some(&sage_snapshot),
            json!({"engine": "Open Sage", "plies": 2, "candidates": [candidate(2)]}),
        ),
    ] {
        sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,engine_snapshot,result) VALUES($1,$2,$3,$4,'completed',$5,$6)")
            .bind(id).bind(position_id).bind(kind).bind(plies).bind(snapshot).bind(result)
            .execute(&pool).await.unwrap();
    }
    // Two active GNUbg jobs one level apart must not collide while shifting.
    for plies in [1, 2] {
        sqlx::query("INSERT INTO jobs(id,position_uuid,kind,plies,status,engine_snapshot,engine_config) VALUES($1,$2,'analysis',$3,'queued',$4,'same')")
            .bind(Uuid::new_v4()).bind(position_id).bind(plies).bind(&gnubg_snapshot)
            .execute(&pool).await.unwrap();
    }
    let match_job = Uuid::new_v4();
    let profile = "GNU Backgammon 1.08; 2-ply cubeful deterministic";
    sqlx::query("INSERT INTO jobs(id,match_uuid,kind,plies,status,total_games,engine_snapshot,result) VALUES($1,$2,'match',2,'completed',1,$3,$4)")
        .bind(match_job).bind(match_id).bind(&gnubg_snapshot)
        .bind(json!({"engine": "GNU Backgammon 1.08", "plies": 2, "performance_rating": {"profile": profile}}))
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,1,$2)")
        .bind(match_job)
        .bind(json!({"plies": 2, "performance_rating": {"profile": profile}}))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,1,1,0,'checker',0,$3),($1,$2,1,2,0,'cube',0,$4)")
        .bind(match_job).bind(position_id)
        .bind(json!({"kind": "checker", "checker_candidates": [candidate(2), candidate(0)]}))
        .bind(json!({"kind": "cube", "cube_evaluation": {"unit": "cubeful"}}))
        .execute(&pool).await.unwrap();

    let run = Uuid::new_v4();
    sqlx::query("INSERT INTO engine_match_runs(id,configuration,engines,dice_version,total_matches) VALUES($1,$2,'{}','test',2)")
        .bind(run)
        .bind(json!({"a": {"engine_id": "bgsage", "plies": 3}, "b": {"engine_id": "gnubg", "plies": 2}, "count": 2}))
        .execute(&pool).await.unwrap();

    let session = Uuid::new_v4();
    sqlx::query("INSERT INTO play_sessions(id,state,gnubg_path,sgf,native_id) VALUES($1,$2,'gnubg','','native')")
        .bind(session)
        .bind(json!({"engine": {"id": "gnubg", "version": "test", "plies": 0}, "phase": "move"}))
        .execute(&pool).await.unwrap();

    let card = Uuid::new_v4();
    let imported_card = Uuid::new_v4();
    sqlx::query("INSERT INTO learning_cards(id,signature,payload) VALUES($1,'native',$2),($3,'imported',$4)")
        .bind(card)
        .bind(json!({"solution": {"engine": "GNU Backgammon 1.08", "plies": 2, "candidates": [candidate(2)], "decision": null}}))
        .bind(imported_card)
        .bind(json!({"solution": {"engine": "GNU Backgammon 1.08", "plies": 0, "candidates": [], "imported": gnubg_record}}))
        .execute(&pool).await.unwrap();

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let settings: (i32, i32) = sqlx::query_as("SELECT analysis_plies,play_plies FROM app_settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(settings, (3, 1));

    let job = |id| {
        json_of(
            &pool,
            "SELECT jsonb_build_object('plies',plies,'result',result) FROM jobs WHERE id=$1",
            id,
        )
    };
    let analysis = job(analysis_job).await;
    assert_eq!(analysis["plies"], 3);
    assert_eq!(analysis["result"]["plies"], 3);
    assert_eq!(analysis["result"]["candidates"][0]["plies"], 3);
    assert_eq!(analysis["result"]["candidates"][1]["plies"], 1);
    assert_eq!(analysis["result"]["candidates"][1]["notation"], "8/5 6/5");
    let dice = job(dice_job).await;
    assert_eq!(
        (dice["plies"].clone(), dice["result"]["plies"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(dice["result"]["rolls"][0]["best"]["plies"], 1);
    assert_eq!(job(legacy_job).await["plies"], 2);
    let sage = job(sage_job).await;
    assert_eq!(sage["plies"], 2);
    assert_eq!(sage["result"]["candidates"][0]["plies"], 2);
    let matched = job(match_job).await;
    assert_eq!(matched["plies"], 3);
    let shifted = "GNU Backgammon 1.08; 3-ply cubeful deterministic";
    assert_eq!(matched["result"]["performance_rating"]["profile"], shifted);
    let active: Vec<i32> =
        sqlx::query_scalar("SELECT plies FROM jobs WHERE engine_config='same' ORDER BY plies")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(active, [2, 3]);

    let game = json_of(
        &pool,
        "SELECT summary FROM match_analysis_games WHERE job_id=$1",
        match_job,
    )
    .await;
    assert_eq!(game["plies"], 3);
    assert_eq!(game["performance_rating"]["profile"], shifted);
    let moves: Vec<Value> =
        sqlx::query_scalar("SELECT data FROM move_analyses WHERE job_id=$1 ORDER BY decision_no")
            .bind(match_job)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(moves[0]["checker_candidates"][0]["plies"], 3);
    assert_eq!(moves[0]["checker_candidates"][1]["plies"], 1);
    assert_eq!(
        moves[1],
        json!({"kind": "cube", "cube_evaluation": {"unit": "cubeful"}})
    );

    let configuration = json_of(
        &pool,
        "SELECT configuration FROM engine_match_runs WHERE id=$1",
        run,
    )
    .await;
    assert_eq!(
        configuration["a"]["plies"], 3,
        "Open Sage already used the convention"
    );
    assert_eq!(configuration["b"]["plies"], 3);
    assert_eq!(configuration["count"], 2);

    let state = json_of(
        &pool,
        "SELECT state FROM play_sessions WHERE id=$1",
        session,
    )
    .await;
    assert_eq!(state["engine"]["plies"], 1);
    // Sessions saved before the depth was recorded analysed at GNUbg's native 2-ply.
    assert_eq!(state["analysis_plies"], 3);

    let gnubg = json_of(
        &pool,
        "SELECT data FROM imported_analyses WHERE id=$1",
        gnubg_import,
    )
    .await;
    assert_eq!(gnubg["depth_plies"], 1);
    assert_eq!(gnubg["depth"], "1-ply cubeful");
    assert_eq!(gnubg["candidates"][0]["depth"], "3-ply cubeful");
    assert_eq!(gnubg["candidates"][0]["depth_plies"], 3);
    assert_eq!(gnubg["candidates"][1]["depth"], "1-ply cubeless");
    assert_eq!(
        gnubg["candidates"][0]["raw"],
        gnubg_record["candidates"][0]["raw"]
    );
    assert_eq!(gnubg["raw"], gnubg_record["raw"]);
    let xg = json_of(
        &pool,
        "SELECT data FROM imported_analyses WHERE id=$1",
        xg_import,
    )
    .await;
    assert_eq!(xg, xg_record);

    let native_card = json_of(
        &pool,
        "SELECT payload FROM learning_cards WHERE id=$1",
        card,
    )
    .await;
    assert_eq!(native_card["solution"]["plies"], 3);
    assert_eq!(native_card["solution"]["candidates"][0]["plies"], 3);
    assert_eq!(native_card["solution"]["decision"], Value::Null);
    let source_card = json_of(
        &pool,
        "SELECT payload FROM learning_cards WHERE id=$1",
        imported_card,
    )
    .await;
    assert_eq!(source_card["solution"]["plies"], 1);
    assert_eq!(source_card["solution"]["imported"]["depth_plies"], 1);
}
