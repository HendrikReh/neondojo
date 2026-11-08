//! Durable per-game checkpoints. All writes are fenced by the worker lease.
use crate::{api::AppState, match_analysis::AnalyzedGame, model::*};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub(crate) async fn execute(
    state: &AppState,
    job: &Job,
    token: Uuid,
    engine: &crate::engine::Engine,
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
            engine: engine.version().await?,
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
    for &game_no in &game_numbers[job.completed_games as usize..] {
        let analyzed = engine
            .analyze_game(&source, game_no, job.plies)
            .await
            .with_context(|| {
                format!(
                    "GNUbg analysis failed in game {game_no}/{}",
                    job.total_games
                )
            })?;
        checkpoint(&state.pool, job, token, &analyzed, &mut summary).await?;
    }
    ensure!(
        summary.analyzed_moves == total_moves,
        "GNUbg analyzed {} of {total_moves} checker moves",
        summary.analyzed_moves
    );
    Ok(serde_json::to_value(summary)?)
}

async fn assert_lease(tx: &mut Transaction<'_, Postgres>, id: Uuid, token: Uuid) -> Result<()> {
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
    let mut tx = pool.begin().await?;
    assert_lease(&mut tx, job.id, token).await?;
    for (index, decision) in game.decisions.iter().enumerate() {
        ensure!(
            decision.decision_no == index as i32 + 1 && (0..=1).contains(&decision.player),
            "Invalid GNUbg decision order/player"
        );
        ensure!(
            decision.equity_loss.is_finite() && decision.equity_loss >= 0.0,
            "Invalid equity loss"
        );
        let position_uuid = if decision.kind == "checker" {
            Some(positions[decision.checker_no.context("Missing checker index")? as usize - 1].id)
        } else {
            None
        };
        let player = &mut next.players[decision.player as usize];
        match decision.kind.as_str() {
            "checker" => {
                player.checker_decisions += 1;
                player.checker_equity_loss += decision.equity_loss;
            }
            "cube" => {
                player.cube_decisions += 1;
                player.cube_equity_loss += decision.equity_loss;
            }
            _ => anyhow::bail!("Unknown analysis decision kind"),
        }
        if decision.evaluation_status == "forced" {
            player.forced_moves += 1;
        }
        if decision.evaluation_status == "unavailable" {
            player.unavailable += 1;
        }
        match decision.severity.as_str() {
            "doubtful" => player.doubtful += 1,
            "bad" => player.bad += 1,
            "very_bad" => player.very_bad += 1,
            "none" => {}
            _ => anyhow::bail!("Unknown analysis error severity"),
        }
        sqlx::query("INSERT INTO move_analyses(job_id,position_uuid,game_no,decision_no,player,kind,equity_loss,data) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(job.id).bind(position_uuid).bind(game.game_no).bind(decision.decision_no).bind(decision.player).bind(&decision.kind)
            .bind(decision.equity_loss).bind(serde_json::to_value(decision)?).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO match_analysis_games(job_id,game_no) VALUES($1,$2)")
        .bind(job.id)
        .bind(game.game_no)
        .execute(&mut *tx)
        .await?;
    let progress = next.completed_games * 100 / job.total_games;
    sqlx::query("UPDATE jobs SET completed_games=$3,progress=$4,result=$5,updated_at=now(),lease_until=now()+interval '120 seconds' WHERE id=$1 AND lease_token=$2")
        .bind(job.id).bind(token).bind(next.completed_games).bind(progress).bind(serde_json::to_value(&next)?).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'progress',$2)")
        .bind(job.id).bind(json!({"status":"running","progress":progress,"total_games":job.total_games,"completed_games":next.completed_games}))
        .execute(&mut *tx).await?;
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
        player.value =
            (decisions > 0).then(|| 500.0 * player.normalized_equity_loss / decisions as f64);
    }
    rating.games += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::Engine, import, jobs, store};

    fn rating_summary() -> MatchAnalysisSummary {
        MatchAnalysisSummary {
            engine: "GNU Backgammon test".into(),
            plies: 0,
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
        let job = store::create_match_job_scoped(&pool, imported.r#match.id, 0, 1, Some(2))
            .await
            .unwrap();
        let token = Uuid::new_v4();
        sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(job.id).bind(token).execute(&pool).await.unwrap();
        let mut summary = MatchAnalysisSummary {
            engine: engine.version().await.unwrap(),
            plies: 0,
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
        let game = engine.analyze_game(sgf, 2, 0).await.unwrap();
        let error = checkpoint(&pool, &job, Uuid::new_v4(), &game, &mut summary)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("lease"), "{error}");
        assert_eq!(summary.completed_games, 0);
        checkpoint(&pool, &job, token, &game, &mut summary)
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
        let job = store::create_match_job(&pool, imported.r#match.id, 0, 2)
            .await
            .unwrap();
        let token = Uuid::new_v4();
        sqlx::query("UPDATE jobs SET status='running',attempts=1,lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(job.id).bind(token).execute(&pool).await.unwrap();
        let mut summary = MatchAnalysisSummary {
            engine: engine.version().await.unwrap(),
            plies: 0,
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
        let first = engine.analyze_game(&sgf, 1, 0).await.unwrap();
        checkpoint(&pool, &job, token, &first, &mut summary)
            .await
            .unwrap();
        let ids: Vec<i64> =
            sqlx::query_scalar("SELECT id FROM move_analyses WHERE job_id=$1 ORDER BY id")
                .bind(job.id)
                .fetch_all(&pool)
                .await
                .unwrap();
        let second = engine.analyze_game(&sgf, 2, 0).await.unwrap();
        let mut stale = summary.clone();
        assert!(
            checkpoint(&pool, &job, Uuid::new_v4(), &second, &mut stale)
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
            checkpoint(&pool, &job, token, &second, &mut stale)
                .await
                .is_err()
        );
        let state = AppState::new(pool.clone(), engine);
        assert!(jobs::run_one(&state).await.unwrap());
        let done = store::get_job(&pool, job.id).await.unwrap().unwrap();
        assert_eq!(done.status, "completed", "{:?}", done.error);
        assert_eq!(done.attempts, 2);
        assert_eq!(done.completed_games, 2);
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
            0,
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
            plies: 0,
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
            .analyze_game(sgf, 1, 0)
            .await
            .unwrap();
        checkpoint(&pool, &job, token, &game, &mut summary)
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
