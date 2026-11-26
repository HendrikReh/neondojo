use super::super::{Competitor, DiceMode, DuelFormat, SeriesConfig, SeriesEngines, dice, runner};
use crate::{engines::SageSnapshot, settings::EngineSnapshot};
use sqlx::PgPool;
use std::{os::unix::fs::PermissionsExt, process::Stdio, time::Duration};
use tokio::{
    process::Command,
    time::{sleep, timeout},
};
use uuid::Uuid;

// Run both profile workers through begin_batch and then stall GNUbg inside
// the opening request. This leaves all three actual children owned by the game.
#[sqlx::test(migrations = "./migrations")]
async fn lost_lease_reaps_all_duel_children_before_any_checkpoint(pool: PgPool) {
    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("engine");
    std::fs::write(
        &executable,
        r#"#!/usr/bin/env python3
import json, os, sys, time
from pathlib import Path
root = Path(__file__).parent
if '--version' in sys.argv:
    print('GNU Backgammon test')
elif '-t' in sys.argv:
    print('GNU Backgammon test', flush=True)
    print('{"ready":true}', flush=True)
    sys.stdin.readline()
    (root / 'rules').write_text(str(os.getpid()))
    time.sleep(60)
else:
    for line in sys.stdin:
        request = json.loads(line)
        if request['operation'] == 'initialize':
            profile = request['profile']
            (root / str(profile['plies'])).write_text(str(os.getpid()))
        elif request['operation'] != 'begin_batch':
            raise ValueError('unexpected evaluation')
        identity = {k: profile[k] for k in ('package_version', 'artifact_fingerprint')}
        print(json.dumps({'id': request['id'], 'result': identity}), flush=True)
"#,
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let snapshot = |plies| EngineSnapshot {
        engine_id: "bgsage".into(),
        gnubg_path: executable.to_str().unwrap().into(),
        engine_version: None,
        gnubg_threads: None,
        gnubg_move_filter: None,
        open_sage: Some(SageSnapshot {
            python: executable.to_str().unwrap().into(),
            package_version: "3.1.2".into(),
            artifact_fingerprint: "a".repeat(64),
            threads: 2,
            plies,
            adapter_version: 1,
            one_ply_verified: false,
            cubeless: false,
        }),
    };
    let competitor = |plies| Competitor {
        engine_id: "bgsage".into(),
        plies,
    };
    let config = SeriesConfig {
        a: competitor(2),
        b: competitor(3),
        format: DuelFormat::Match,
        match_length: 3,
        block_games: None,
        count: 1,
        mode: DiceMode::Random,
        seed: Uuid::nil(),
    };
    let engines = SeriesEngines {
        rules_version: "GNU Backgammon test".into(),
        a: snapshot(2),
        b: snapshot(3),
    };
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO engine_match_runs(id,configuration,engines,dice_version,status,total_matches) VALUES($1,$2,$3,$4,'queued',1)")
        .bind(id).bind(sqlx::types::Json(config)).bind(sqlx::types::Json(engines)).bind(dice::VERSION).execute(&pool).await.unwrap();
    let worker = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(150))
        .connect_with((*pool.connect_options()).clone())
        .await
        .unwrap();
    let run = runner::run_one(&worker);
    tokio::pin!(run);
    tokio::select! {
        result = &mut run => panic!("worker exited before losing its lease: {result:?}"),
        result = timeout(Duration::from_secs(10), async {
            while !directory.path().join("rules").is_file() { sleep(Duration::from_millis(10)).await; }
        }) => result.expect("worker never started the opening request"),
    }

    let held = worker.acquire().await.unwrap();
    let original: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT lease_until FROM engine_match_runs WHERE id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let delay_and_recover = async {
        sleep(Duration::from_millis(1500)).await;
        drop(held);
        while !sqlx::query_scalar::<_, bool>(
            "SELECT lease_until>$2 FROM engine_match_runs WHERE id=$1",
        )
        .bind(id)
        .bind(original)
        .fetch_one(&pool)
        .await
        .unwrap()
        {
            sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::select! {
        result = &mut run => panic!("duel stopped during a transient pool timeout: {result:?}"),
        result = timeout(Duration::from_secs(5), delay_and_recover) => result.unwrap(),
    }

    sqlx::query("UPDATE engine_match_runs SET lease_until=now()-interval '1 second' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(timeout(Duration::from_secs(5), run).await.unwrap().unwrap());
    for file in ["2", "3", "rules"] {
        let pid = std::fs::read_to_string(directory.path().join(file)).unwrap();
        timeout(Duration::from_secs(3), async {
            while Command::new("/bin/kill")
                .args(["-0", &pid])
                .stderr(Stdio::null())
                .status()
                .await
                .unwrap()
                .success()
            {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("old worker must reap every child");
    }
    let state: (String, i32, i64, i64) = sqlx::query_as(
        "SELECT status, completed_matches, (SELECT count(*) FROM engine_match_results WHERE run_id=$1),
         (SELECT count(*) FROM matches WHERE database_id=$2) FROM engine_match_runs WHERE id=$1"
    ).bind(id).bind(runner::ENGINE_DUELS_DATABASE_ID).fetch_one(&pool).await.unwrap();
    assert_eq!(
        state,
        ("running".into(), 0, 0, 0),
        "only the next lease owner may recover this match"
    );
}
