//! Durable per-game checkpoints. All writes are fenced by the worker lease.
use crate::{api::AppState, match_analysis::AnalyzedGame, model::*};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use std::time::Instant;
use uuid::Uuid;

#[cfg(test)]
mod checkpoint_tests;

const CHECKPOINT_BATCH_SIZE: usize = 500;

pub(crate) async fn execute(
    state: &AppState,
    job: &Job,
    token: Uuid,
    engine: &crate::engines::AnalysisEngine,
) -> Result<Value> {
    let match_id = job.match_uuid.context("Match job has no match target")?;
    let source: String = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1")
        .bind(match_id)
        .fetch_one(&state.pool)
        .await?;
    let total_moves: i32 = sqlx::query_scalar("SELECT count(*)::integer FROM positions WHERE match_uuid=$1 AND ($2::integer IS NULL OR game_no=$2)")
        .bind(match_id).bind(job.game_no)
        .fetch_one(&state.pool)
        .await?;
    let mut summary: MatchAnalysisSummary = if let Some(value) = &job.result {
        serde_json::from_value(value.clone()).context("Invalid saved match checkpoint")?
    } else {
        MatchAnalysisSummary {
            // A submitted job already owns its immutable identity. Initialize
            // waiting timing before the gate; each native call still validates it.
            engine: match engine.saved_version() {
                Some(version) => version.to_owned(),
                None => engine.version().await?,
            },
            plies: job.plies,
            total_games: job.total_games,
            completed_games: 0,
            total_moves,
            analyzed_moves: 0,
            performance_rating: None,
            players: [
                PlayerAnalysisSummary::default(),
                PlayerAnalysisSummary {
                    player: 1,
                    ..Default::default()
                },
            ],
        }
    };
    let completed: Vec<i32> = sqlx::query_scalar(
        "SELECT game_no FROM match_analysis_games WHERE job_id=$1 ORDER BY game_no",
    )
    .bind(job.id)
    .fetch_all(&state.pool)
    .await?;
    let game_numbers: Vec<i32> = if let Some(game_no) = job.game_no {
        vec![game_no]
    } else {
        (1..=job.total_games).collect()
    };
    ensure!(
        completed == game_numbers[..job.completed_games as usize]
            && summary.completed_games == job.completed_games,
        "Inconsistent match analysis checkpoint"
    );
    crate::analysis_timing::initialize(
        &state.pool,
        job,
        token,
        engine,
        &summary.engine,
        total_moves,
        summary.analyzed_moves,
    )
    .await?;
    for &game_no in &game_numbers[job.completed_games as usize..] {
        crate::engine::with_shared_access(async {
            crate::analysis_timing::start_game(&state.pool, job, token).await?;
            let started = Instant::now();
            let work = async {
                let analyzed = engine
                    .analyze_game(&source, game_no, job.plies)
                    .await
                    .with_context(|| {
                        format!(
                            "{} analysis failed in game {game_no}/{}",
                            engine.id().display_name(),
                            job.total_games
                        )
                    })?;
                checkpoint(
                    &state.pool,
                    job,
                    token,
                    &analyzed,
                    &mut summary,
                    Some(started),
                )
                .await
            };
            let samples = async {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    crate::analysis_timing::sample_game(
                        &state.pool,
                        job,
                        token,
                        started.elapsed().as_secs_f64() * 1000.0,
                    )
                    .await?;
                }
            };
            tokio::select! {
                result = work => result,
                result = samples => result,
            }
        })
        .await?;
    }
    ensure!(
        summary.analyzed_moves == total_moves,
        "GNUbg analyzed {} of {total_moves} checker moves",
        summary.analyzed_moves
    );
    Ok(serde_json::to_value(summary)?)
}

pub(crate) async fn assert_lease(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    token: Uuid,
) -> Result<()> {
    let owns: Option<Uuid> = sqlx::query_scalar("SELECT id FROM jobs WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now() FOR UPDATE")
        .bind(id).bind(token).fetch_optional(&mut **tx).await?;
    ensure!(owns.is_some(), "Job lease was lost");
    Ok(())
}

async fn checkpoint(
    pool: &PgPool,
    job: &Job,
    token: Uuid,
    game: &AnalyzedGame,
    summary: &mut MatchAnalysisSummary,
    started: Option<Instant>,
) -> Result<()> {
    ensure!(
        game.game_no == job.game_no.unwrap_or(summary.completed_games + 1)
            && summary.completed_games < job.total_games,
        "Unexpected analyzed game order"
    );
    let positions: Vec<Position> = sqlx::query_as(
        "SELECT * FROM positions WHERE match_uuid=$1 AND game_no=$2 ORDER BY ordinal",
    )
    .bind(job.match_uuid)
    .bind(game.game_no)
    .fetch_all(pool)
    .await?;
    let checkers: Vec<_> = game
        .decisions
        .iter()
        .filter(|d| d.kind == "checker")
        .collect();
    ensure!(
        checkers.len() == positions.len(),
        "Game {}: GNUbg returned {} of {} checker decisions",
        game.game_no,
        checkers.len(),
        positions.len()
    );
    for (index, (decision, position)) in checkers.iter().zip(&positions).enumerate() {
        ensure!(
            decision.checker_no == Some(index as i32 + 1)
                && decision.player == position.player
                && decision.dice == Some([position.dice[0], position.dice[1]]),
            "Game {}: GNUbg checker decision {} does not match the imported game",
            game.game_no,
            index + 1
        );
    }
    let mut next = summary.clone();
    accumulate_performance_rating(&mut next, game);
    next.engine.clone_from(&game.engine);
    next.completed_games += 1;
    next.analyzed_moves += positions.len() as i32;
    let mut game_summary = MatchAnalysisSummary {
        engine: game.engine.clone(),
        plies: job.plies,
        total_games: 1,
        completed_games: 0,
        total_moves: positions.len() as i32,
        analyzed_moves: positions.len() as i32,
        players: [
            PlayerAnalysisSummary::default(),
            PlayerAnalysisSummary {
                player: 1,
                ..Default::default()
            },
        ],
        performance_rating: None,
    };
    accumulate_performance_rating(&mut game_summary, game);
    game_summary.completed_games = 1;
    // Validate and encode the game before locking the lease row. Heartbeats
    // only have to wait for the transaction that publishes the checkpoint.
    let mut inserts = Vec::new();
    for (batch_index, batch) in game.decisions.chunks(CHECKPOINT_BATCH_SIZE).enumerate() {
        let mut rows = Vec::with_capacity(batch.len());
        for (index, decision) in batch.iter().enumerate() {
            let number = batch_index * CHECKPOINT_BATCH_SIZE + index + 1;
            ensure!(
                decision.decision_no == number as i32 && (0..=1).contains(&decision.player),
                "Invalid GNUbg decision order/player"
            );
            ensure!(
                decision.equity_loss.is_finite() && decision.equity_loss >= 0.0,
                "Invalid equity loss"
            );
            let position_uuid = if decision.kind == "checker" {
                Some(
                    positions[decision.checker_no.context("Missing checker index")? as usize - 1]
                        .id,
                )
            } else {
                None
            };
            crate::match_analysis::accumulate_decision(&mut next.players, decision)?;
            crate::match_analysis::accumulate_decision(&mut game_summary.players, decision)?;
            rows.push((position_uuid, decision, serde_json::to_value(decision)?));
        }
        let mut insert = QueryBuilder::<Postgres>::new(
            "INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) ",
        );
        insert.push_values(rows, |mut row, (position_uuid, decision, data)| {
            row.push_bind(job.id)
                .push_bind(position_uuid)
                .push_bind(game.game_no)
                .push_bind(decision.decision_no)
                .push_bind(decision.player)
                .push_bind(&decision.kind)
                .push_bind(decision.equity_loss)
                .push_bind(data);
        });
        inserts.push(insert);
    }
    let game_summary = serde_json::to_value(game_summary)?;
    let result = serde_json::to_value(&next)?;
    let progress = next.completed_games * 100 / job.total_games;
    let progress_event = json!({
        "status": "running",
        "progress": progress,
        "total_games": job.total_games,
        "completed_games": next.completed_games,
    });
    let mut tx = pool.begin().await?;
    assert_lease(&mut tx, job.id, token).await?;
    for insert in &mut inserts {
        // Variable row counts produce different SQL and would churn the cache.
        insert.build().persistent(false).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,$2,$3)")
        .bind(job.id)
        .bind(game.game_no)
        .bind(game_summary)
        .execute(&mut *tx)
        .await?;
    if let Some(started) = started {
        crate::analysis_timing::confirm_game(
            &mut tx,
            job,
            token,
            positions.len() as i64,
            started.elapsed().as_secs_f64() * 1000.0,
        )
        .await?;
    }
    sqlx::query("UPDATE jobs SET completed_games=$3,progress=$4,result=$5,updated_at=now(),lease_until=now()+interval '120 seconds' WHERE id=$1 AND lease_token=$2")
        .bind(job.id).bind(token).bind(next.completed_games).bind(progress).bind(result).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'progress',$2)")
        .bind(job.id)
        .bind(progress_event)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    *summary = next;
    Ok(())
}

fn accumulate_performance_rating(summary: &mut MatchAnalysisSummary, game: &AnalyzedGame) {
    // A resumed legacy checkpoint cannot retroactively prove earlier denominators.
    let Some(statistics) = &game.statistics else {
        summary.performance_rating = None;
        return;
    };
    if summary.completed_games == 0 {
        summary.performance_rating = Some(PerformanceRatingSummary {
            method: crate::match_analysis::PERFORMANCE_RATING_METHOD.into(),
            profile: crate::match_analysis::performance_rating_profile(&game.engine, summary.plies),
            games: 0,
            players: [
                PlayerPerformanceRating::default(),
                PlayerPerformanceRating {
                    player: 1,
                    ..Default::default()
                },
            ],
        });
    }
    let Some(rating) = &mut summary.performance_rating else {
        return;
    };
    if rating.games != summary.completed_games
        || rating.method != crate::match_analysis::PERFORMANCE_RATING_METHOD
        || rating.profile
            != crate::match_analysis::performance_rating_profile(&game.engine, summary.plies)
    {
        summary.performance_rating = None;
        return;
    }
    for (player, native) in rating.players.iter_mut().zip(statistics) {
        player.normalized_equity_loss += native.normalized_equity_loss;
        player.unforced_checker_moves += native.unforced_checker_moves;
        player.close_cube_decisions += native.close_cube_decisions;
        let decisions = player.unforced_checker_moves + player.close_cube_decisions;
        player.value = crate::match_analysis::performance_rating_value(
            player.normalized_equity_loss,
            i64::from(decisions),
        );
    }
    rating.games += 1;
}

#[cfg(test)]
mod tests {
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
            crate::match_analysis::match_performance_rating(
                &serde_json::to_value(&summary).unwrap()
            )
            .is_none()
        );
        accumulate_performance_rating(&mut summary, &rated_game([0.06, 0.03], [8, 0], [1, 2]));
        summary.completed_games = 2;
        let complete = crate::match_analysis::match_performance_rating(
            &serde_json::to_value(&summary).unwrap(),
        )
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
    async fn scoped_checkpoint_recovery_keeps_source_game_and_completed_count_separate(
        pool: PgPool,
    ) {
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
        sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(job.id).bind(token).execute(&pool).await.unwrap();
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
        let sgf = engine.normalize_mat(mat).await.unwrap();
        let parsed = import::parse_sgf(&sgf).unwrap();
        let imported = store::import_match(&pool, "resume.mat", "mat", mat, &sgf, &parsed)
            .await
            .unwrap();
        let job = store::create_match_job(&pool, imported.r#match.id, 1, 2)
            .await
            .unwrap();
        let token = Uuid::new_v4();
        sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(job.id).bind(token).execute(&pool).await.unwrap();
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
        let timing: (i64, i64, f64) = sqlx::query_as("SELECT completed_moves,timed_moves,measured_elapsed_ms FROM match_analysis_timing WHERE job_id=$1").bind(job.id).fetch_one(&pool).await.unwrap();
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
    async fn resumed_snapshot_rejects_a_new_version_without_mixing_checkpoint_results(
        pool: PgPool,
    ) {
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
        sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(job.id).bind(token).execute(&pool).await.unwrap();
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
}
