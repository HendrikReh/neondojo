use super::*;
use crate::{engine::Engine, import, jobs, store};

#[sqlx::test(migrations = "./migrations")]
async fn saved_snapshot_is_ready_while_waiting_for_the_engine_gate(pool: PgPool) {
    let sgf = "(;GM[6]FF[4]MI[length:7][ws:0][bs:0];W[31hefe])";
    let imported = store::import_match(
        &pool,
        "gate.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let snapshot = crate::settings::EngineSnapshot {
        gnubg_threads: None,
        gnubg_move_filter: None,
        open_sage: None,
        engine_id: "gnubg".into(),
        gnubg_path: "/missing-native-engine".into(),
        engine_version: Some("GNU Backgammon saved".into()),
    };
    let job = store::create_match_job_scoped_with_snapshot(
        &pool,
        imported.r#match.id,
        1,
        1,
        None,
        Some(&snapshot),
    )
    .await
    .unwrap();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let exclusive = tokio::spawn(crate::engine::benchmark_exclusive(async move {
        entered_tx.send(()).unwrap();
        release_rx.await.unwrap();
    }));
    entered_rx.await.unwrap();
    let state = AppState::new(pool.clone(), Engine::new("/unused-fallback".into()));
    let worker = tokio::spawn(async move { jobs::run_one(&state).await.unwrap() });
    let ready=tokio::time::timeout(std::time::Duration::from_secs(1),async {
        loop {
            let sample: Option<(Option<chrono::DateTime<chrono::Utc>>,f64)>=sqlx::query_as("SELECT active_started_at,measured_elapsed_ms FROM match_analysis_timing WHERE job_id=$1").bind(job.id).fetch_optional(&pool).await.unwrap();
            if let Some(sample)=sample { break sample; }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await;
    release_tx.send(()).unwrap();
    exclusive.await.unwrap();
    worker.await.unwrap();
    assert_eq!(
        ready.expect(
            "saved engine identity should initialize timing without waiting on native work"
        ),
        (None, 0.0)
    );
    let done = store::get_job(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(done.status, "failed");
    assert_eq!(
        sqlx::query_scalar::<_, f64>(
            "SELECT measured_elapsed_ms FROM match_analysis_timing WHERE job_id=$1"
        )
        .bind(job.id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0.0,
        "failed partial sections must not add measured time"
    );
}

fn rating_summary() -> MatchAnalysisSummary {
    MatchAnalysisSummary {
        engine: "GNU Backgammon test".into(),
        plies: 1,
        total_games: 2,
        completed_games: 0,
        total_moves: 0,
        analyzed_moves: 0,
        players: [
            PlayerAnalysisSummary::default(),
            PlayerAnalysisSummary {
                player: 1,
                ..Default::default()
            },
        ],
        performance_rating: None,
    }
}

fn rated_game(loss: [f64; 2], checker: [i32; 2], cube: [i32; 2]) -> AnalyzedGame {
    AnalyzedGame {
        cube_enabled: true,
        engine: "GNU Backgammon test".into(),
        game_no: 1,
        decisions: Vec::new(),
        statistics: Some(std::array::from_fn(|p| {
            crate::match_analysis::NativePlayerStatistics {
                total_checker_moves: checker[p],
                total_cube_decisions: cube[p],
                unforced_checker_moves: checker[p],
                close_cube_decisions: cube[p],
                normalized_equity_loss: loss[p],
            }
        })),
    }
}

#[test]
fn performance_rating_weights_native_decisions_and_keeps_zero_denominator_null() {
    let mut summary = rating_summary();
    accumulate_performance_rating(&mut summary, &rated_game([0.04, 0.0], [1, 0], [0, 0]));
    let first = summary.performance_rating.as_ref().unwrap();
    assert_eq!(first.players[0].value, Some(20.0));
    assert_eq!(first.players[1].value, None);
    summary.completed_games = 1;
    // A persisted checkpoint must retain native sums and counts, not an average PR.
    let mut summary: MatchAnalysisSummary =
        serde_json::from_value(serde_json::to_value(summary).unwrap()).unwrap();
    assert!(
        crate::match_analysis::match_performance_rating(&serde_json::to_value(&summary).unwrap())
            .is_none()
    );
    accumulate_performance_rating(&mut summary, &rated_game([0.06, 0.03], [8, 0], [1, 2]));
    summary.completed_games = 2;
    let complete =
        crate::match_analysis::match_performance_rating(&serde_json::to_value(&summary).unwrap())
            .unwrap();
    assert_eq!(complete.games, 2);
    assert_eq!(complete.players[0].value, Some(5.0));
    assert_eq!(complete.players[0].unforced_checker_moves, 9);
    assert_eq!(complete.players[0].close_cube_decisions, 1);
    assert_eq!(complete.players[1].value, Some(7.5));
}

#[test]
fn performance_rating_never_fills_legacy_or_missing_checkpoint_statistics() {
    let mut legacy = serde_json::to_value(rating_summary()).unwrap();
    legacy.as_object_mut().unwrap().remove("performance_rating");
    legacy["completed_games"] = json!(1);
    legacy["players"][0]["checker_equity_loss"] = json!(12.0);
    let mut legacy: MatchAnalysisSummary = serde_json::from_value(legacy).unwrap();
    accumulate_performance_rating(&mut legacy, &rated_game([0.0, 0.0], [2, 2], [0, 0]));
    assert!(legacy.performance_rating.is_none());

    let mut summary = rating_summary();
    accumulate_performance_rating(&mut summary, &rated_game([0.0, 0.0], [1, 1], [0, 0]));
    summary.completed_games = 1;
    let mut missing = rated_game([0.0, 0.0], [1, 1], [0, 0]);
    missing.statistics = None;
    accumulate_performance_rating(&mut summary, &missing);
    assert!(summary.performance_rating.is_none());
    summary.completed_games = 2;
    accumulate_performance_rating(&mut summary, &rated_game([0.0, 0.0], [1, 1], [0, 0]));
    assert!(summary.performance_rating.is_none());
}

#[test]
fn performance_rating_helper_requires_complete_consistent_evaluation() {
    let mut summary = rating_summary();
    summary.total_games = 1;
    accumulate_performance_rating(&mut summary, &rated_game([0.0, 0.0], [0, 0], [0, 0]));
    summary.completed_games = 1;
    let valid = serde_json::to_value(summary).unwrap();
    let rating = crate::match_analysis::match_performance_rating(&valid).unwrap();
    assert!(rating.players.iter().all(|p| p.value.is_none()));
    for (pointer, value) in [
        ("/completed_games", json!(0)),
        ("/analyzed_moves", json!(1)),
        ("/players/0/unavailable", json!(1)),
        ("/performance_rating/games", json!(0)),
        ("/performance_rating/method", json!("xg")),
        ("/performance_rating/profile", json!("unknown")),
        ("/performance_rating/players/0/value", json!(0.0)),
        (
            "/performance_rating/players/0/unforced_checker_moves",
            json!(-1),
        ),
    ] {
        let mut invalid = valid.clone();
        *invalid.pointer_mut(pointer).unwrap() = value;
        assert!(
            crate::match_analysis::match_performance_rating(&invalid).is_none(),
            "{invalid}"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn scoped_checkpoint_recovery_keeps_source_game_and_completed_count_separate(pool: PgPool) {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let sgf = "(;GM[6]FF[4]MI[length:7][ws:0][bs:0];W[31hefe])\n(;GM[6]FF[4]MI[length:7][ws:1][bs:0];W[31hefe])";
    let imported = store::import_match(
        &pool,
        "scoped-resume.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let job = store::create_match_job_scoped(&pool, imported.r#match.id, 1, 1, Some(2))
        .await
        .unwrap();
    let token = Uuid::new_v4();
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+make_interval(secs=>$3) WHERE id=$1")
        .bind(job.id)
        .bind(token)
        .bind(jobs::LEASE.as_secs_f64())
        .execute(&pool)
        .await
        .unwrap();
    let mut summary = MatchAnalysisSummary {
        engine: engine.version().await.unwrap(),
        plies: 1,
        total_games: 1,
        completed_games: 0,
        total_moves: 1,
        analyzed_moves: 0,
        performance_rating: None,
        players: [
            PlayerAnalysisSummary::default(),
            PlayerAnalysisSummary {
                player: 1,
                ..Default::default()
            },
        ],
    };
    let game = engine.analyze_game(sgf, 2, 1).await.unwrap();
    let error = checkpoint(&pool, &job, Uuid::new_v4(), &game, &mut summary, None)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("lease"), "{error}");
    assert_eq!(summary.completed_games, 0);
    checkpoint(&pool, &job, token, &game, &mut summary, None)
        .await
        .unwrap();
    assert_eq!(summary.completed_games, 1);
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM move_analyses WHERE job_id=$1 ORDER BY id")
            .bind(job.id)
            .fetch_all(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE jobs SET lease_until=now()-interval '1 second' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let state = AppState::new(pool.clone(), engine);
    assert!(jobs::run_one(&state).await.unwrap());
    let done = store::get_job(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(done.status, "completed", "{:?}", done.error);
    assert_eq!(
        (done.attempts, done.total_games, done.completed_games),
        (2, 1, 1)
    );
    assert_eq!(done.game_no, Some(2));
    let persisted: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM move_analyses WHERE job_id=$1 ORDER BY id")
            .bind(job.id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        persisted, ids,
        "a committed scoped checkpoint must never be analyzed again"
    );
    let games: Vec<i32> =
        sqlx::query_scalar("SELECT game_no FROM match_analysis_games WHERE job_id=$1")
            .bind(job.id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(games, [2]);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn checkpoints_survive_restarts_and_reject_stale_workers(pool: PgPool) {
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let mat = b"7 point match\n\n Game 1\n Alice : 0                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n\n Game 2\n Alice : 1                         Bob : 0\n 1) 31: 8/5 6/5                    42: 8/4 6/4\n";
    let sgf = crate::import::mat::normalize_mat(&engine, mat)
        .await
        .unwrap();
    let parsed = import::parse_sgf(&sgf).unwrap();
    let imported = store::import_match(&pool, "resume.mat", "mat", mat, &sgf, &parsed)
        .await
        .unwrap();
    let job = store::create_match_job(&pool, imported.r#match.id, 1, 2)
        .await
        .unwrap();
    let token = Uuid::new_v4();
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+make_interval(secs=>$3) WHERE id=$1")
        .bind(job.id)
        .bind(token)
        .bind(jobs::LEASE.as_secs_f64())
        .execute(&pool)
        .await
        .unwrap();
    let mut summary = MatchAnalysisSummary {
        engine: engine.version().await.unwrap(),
        plies: 1,
        total_games: 2,
        completed_games: 0,
        total_moves: 4,
        analyzed_moves: 0,
        performance_rating: None,
        players: [
            PlayerAnalysisSummary::default(),
            PlayerAnalysisSummary {
                player: 1,
                ..Default::default()
            },
        ],
    };
    let first = engine.analyze_game(&sgf, 1, 1).await.unwrap();
    checkpoint(&pool, &job, token, &first, &mut summary, None)
        .await
        .unwrap();
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM move_analyses WHERE job_id=$1 ORDER BY id")
            .bind(job.id)
            .fetch_all(&pool)
            .await
            .unwrap();
    let second = engine.analyze_game(&sgf, 2, 1).await.unwrap();
    let mut stale = summary.clone();
    assert!(
        checkpoint(&pool, &job, Uuid::new_v4(), &second, &mut stale, None)
            .await
            .is_err()
    );
    assert_eq!(stale.completed_games, 1);
    sqlx::query("UPDATE jobs SET lease_until=now()-interval '1 second' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        checkpoint(&pool, &job, token, &second, &mut stale, None)
            .await
            .is_err()
    );
    let state = AppState::new(pool.clone(), engine);
    assert!(jobs::run_one(&state).await.unwrap());
    let done = store::get_job(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(done.status, "completed", "{:?}", done.error);
    assert_eq!(done.attempts, 2);
    assert_eq!(done.completed_games, 2);
    let timing: (i64, i64, f64) = sqlx::query_as(
        "SELECT completed_moves,timed_moves,measured_elapsed_ms FROM match_analysis_timing WHERE job_id=$1",
    )
    .bind(job.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (timing.0, timing.1),
        (4, 2),
        "only the newly completed second game has measured timing"
    );
    assert!(timing.2 > 0.0);
    let result = done.result.as_ref().unwrap();
    assert_eq!(result["performance_rating"]["games"], 2);
    assert_eq!(result["performance_rating"]["method"], "gnubg_emg_500_v1");
    for player in 0..2 {
        let a = &first.statistics.as_ref().unwrap()[player];
        let b = &second.statistics.as_ref().unwrap()[player];
        let decisions = a.unforced_checker_moves
            + b.unforced_checker_moves
            + a.close_cube_decisions
            + b.close_cube_decisions;
        let expected =
            500.0 * (a.normalized_equity_loss + b.normalized_equity_loss) / decisions as f64;
        assert!(
            (result["performance_rating"]["players"][player]["value"]
                .as_f64()
                .unwrap()
                - expected)
                .abs()
                < 1e-10
        );
    }
    let persisted: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM move_analyses WHERE job_id=$1 AND game_no=1 ORDER BY id",
    )
    .bind(job.id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        ids, persisted,
        "completed game must not be recalculated/replaced"
    );
    let events = store::events(&pool, job.id, 0).await.unwrap();
    let progress: Vec<i64> = events
        .iter()
        .map(|e| e.data["progress"].as_i64().unwrap())
        .collect();
    assert!(
        progress.windows(2).all(|pair| pair[0] <= pair[1]),
        "{progress:?}"
    );
}

#[cfg(unix)]
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires PostgreSQL and GNUbg"]
async fn resumed_snapshot_rejects_a_new_version_without_mixing_checkpoint_results(pool: PgPool) {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("gnubg-snapshot");
    let source = std::path::PathBuf::from(std::env::var("GNUBG_BIN").unwrap_or("gnubg".into()));
    let source = if source.components().count() == 1 {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(&source))
            .find(|candidate| {
                candidate.is_file()
                    && candidate
                        .metadata()
                        .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
            })
            .expect("GNUbg must be available on PATH")
    } else {
        source
    };
    std::os::unix::fs::symlink(std::fs::canonicalize(source).unwrap(), &path).unwrap();
    let engine = Engine::new(path.to_string_lossy().into_owned());
    let version = engine.version().await.unwrap();
    let snapshot = crate::settings::EngineSnapshot {
        gnubg_threads: None,
        gnubg_move_filter: None,
        open_sage: None,
        engine_id: "gnubg".into(),
        gnubg_path: engine.binary().into(),
        engine_version: Some(version.clone()),
    };
    let sgf = "(;GM[6]FF[4]MI[length:7][ws:0][bs:0];W[31hefe])\n(;GM[6]FF[4]MI[length:7][ws:1][bs:0];W[31hefe])";
    let imported = store::import_match(
        &pool,
        "version-recovery.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf).unwrap(),
    )
    .await
    .unwrap();
    let job = store::create_match_job_scoped_with_snapshot(
        &pool,
        imported.r#match.id,
        1,
        2,
        None,
        Some(&snapshot),
    )
    .await
    .unwrap();
    let token = Uuid::new_v4();
    sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+make_interval(secs=>$3) WHERE id=$1")
        .bind(job.id)
        .bind(token)
        .bind(jobs::LEASE.as_secs_f64())
        .execute(&pool)
        .await
        .unwrap();
    let mut summary = MatchAnalysisSummary {
        engine: version,
        plies: 1,
        total_games: 2,
        completed_games: 0,
        total_moves: 2,
        analyzed_moves: 0,
        performance_rating: None,
        players: [
            PlayerAnalysisSummary::default(),
            PlayerAnalysisSummary {
                player: 1,
                ..Default::default()
            },
        ],
    };
    let game = snapshot
        .engine()
        .unwrap()
        .analyze_game(sgf, 1, 1)
        .await
        .unwrap();
    checkpoint(&pool, &job, token, &game, &mut summary, None)
        .await
        .unwrap();
    let first_rows: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM move_analyses WHERE job_id=$1 ORDER BY id")
            .bind(job.id)
            .fetch_all(&pool)
            .await
            .unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::write(
        &path,
        "#!/bin/sh\necho 'GNU Backgammon changed-after-checkpoint'\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    sqlx::query("UPDATE jobs SET lease_until=now()-interval '1 second' WHERE id=$1")
        .bind(job.id)
        .execute(&pool)
        .await
        .unwrap();
    let state = AppState::new(pool.clone(), engine);
    assert!(jobs::run_one(&state).await.unwrap());
    let failed = store::get_job(&pool, job.id).await.unwrap().unwrap();
    assert_eq!(failed.status, "failed", "{:?}", failed.error);
    assert!(
        failed.error.as_deref().unwrap().contains("version changed"),
        "{:?}",
        failed.error
    );
    assert_eq!((failed.attempts, failed.completed_games), (2, 1));
    assert_eq!(failed.result.unwrap()["completed_games"], 1);
    let remaining_rows: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM move_analyses WHERE job_id=$1 ORDER BY id")
            .bind(job.id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        first_rows, remaining_rows,
        "old checkpoints must remain intact, without any mixed-version continuation"
    );
}
