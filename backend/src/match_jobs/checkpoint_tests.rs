use super::*;
use crate::{engine::Engine, import, match_analysis::Decision, store};

async fn import_source(pool: &PgPool, sgf: &str) -> Result<Uuid> {
    let imported = store::import_match(
        pool,
        "checkpoint.sgf",
        "sgf",
        sgf.as_bytes(),
        sgf,
        &import::parse_sgf(sgf)?,
    )
    .await?;
    Ok(imported.r#match.id)
}

async fn running_job(pool: &PgPool, match_id: Uuid, game_no: i32) -> Result<(Job, Uuid)> {
    let job = store::create_match_job_scoped(pool, match_id, 1, 1, Some(game_no)).await?;
    let token = Uuid::new_v4();
    let job = sqlx::query_as(
        "UPDATE jobs SET status='running', attempts=1, lease_token=$2,
            lease_until=now()+interval '120 seconds' WHERE id=$1 RETURNING *",
    )
    .bind(job.id)
    .bind(token)
    .fetch_one(pool)
    .await?;
    Ok((job, token))
}

fn summary(game: &AnalyzedGame) -> MatchAnalysisSummary {
    MatchAnalysisSummary {
        engine: game.engine.clone(),
        plies: 1,
        total_games: 1,
        completed_games: 0,
        total_moves: game
            .decisions
            .iter()
            .filter(|d| d.kind == "checker")
            .count() as i32,
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
}

#[sqlx::test(migrations = "./migrations")]
async fn checkpoints_batch_rows_and_roll_back_together(pool: PgPool) -> Result<()> {
    let match_id = import_source(
        &pool,
        "(;GM[6]FF[4]MI[length:7][ws:0][bs:0];W[31hefe])\n(;GM[6]FF[4]MI[length:7][ws:1][bs:0])",
    )
    .await?;
    let position_uuid: Uuid =
        sqlx::query_scalar("SELECT id FROM positions WHERE match_uuid=$1 AND game_no=1")
            .bind(match_id)
            .fetch_one(&pool)
            .await?;
    let (job, token) = running_job(&pool, match_id, 1).await?;
    // One real checker occurrence and enough synthetic cube rows to span batches.
    let game = AnalyzedGame {
        engine: "GNU Backgammon checkpoint fixture".into(),
        game_no: 1,
        statistics: None,
        decisions: (1..=501)
            .map(|decision_no| {
                let checker = decision_no == 1;
                Decision {
                    decision_no,
                    checker_no: checker.then_some(1),
                    player: if checker { 0 } else { decision_no % 2 },
                    kind: if checker { "checker" } else { "cube" }.into(),
                    played: if checker { "8/5 6/5" } else { "no double" }.into(),
                    best: if checker { "8/5 6/5" } else { "no double" }.into(),
                    played_equity: Some(0.1),
                    best_equity: Some(0.1),
                    equity_loss: 0.0,
                    severity: "none".into(),
                    dice: checker.then_some([3, 1]),
                    evaluation_status: "evaluated".into(),
                    checker_candidates: Vec::new(),
                    cube_evaluation: None,
                }
            })
            .collect(),
    };
    let mut result = summary(&game);
    let original = serde_json::to_value(&result)?;
    let original_revision: String =
        sqlx::query_scalar("SELECT revision FROM native_analysis_revisions WHERE job_id=$1")
            .bind(job.id)
            .fetch_one(&pool)
            .await?;
    sqlx::raw_sql(
        "CREATE TABLE checkpoint_insert_counts (id bigint GENERATED ALWAYS AS IDENTITY, rows bigint NOT NULL);
         CREATE FUNCTION count_checkpoint_insert() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
             INSERT INTO checkpoint_insert_counts(rows) SELECT count(*) FROM inserted_rows;
             RETURN NULL;
         END $$;
         CREATE TRIGGER count_checkpoint_insert AFTER INSERT ON move_analyses
           REFERENCING NEW TABLE AS inserted_rows FOR EACH STATEMENT EXECUTE FUNCTION count_checkpoint_insert();
         ALTER TABLE move_analyses ADD CONSTRAINT reject_last_decision CHECK (decision_no < 501);",
    )
    .execute(&pool)
    .await?;

    let error = checkpoint(&pool, &job, token, &game, &mut result, None)
        .await
        .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<sqlx::Error>()
            .and_then(sqlx::Error::as_database_error)
            .and_then(sqlx::error::DatabaseError::code)
            .as_deref(),
        Some("23514")
    );
    assert_eq!(serde_json::to_value(&result)?, original);
    let persisted: (i64, i64, i64, i64, i32, i32, Option<Value>, String) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM move_analyses WHERE job_id=j.id),
                (SELECT count(*) FROM match_analysis_games WHERE job_id=j.id),
                (SELECT count(*) FROM job_events WHERE job_id=j.id AND event='progress'),
                (SELECT count(*) FROM checkpoint_insert_counts),
                j.completed_games, j.progress, j.result, r.revision
         FROM jobs j JOIN native_analysis_revisions r ON r.job_id=j.id WHERE j.id=$1",
    )
    .bind(job.id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(persisted, (0, 0, 0, 0, 0, 0, None, original_revision));
    sqlx::query("ALTER TABLE move_analyses DROP CONSTRAINT reject_last_decision")
        .execute(&pool)
        .await?;

    checkpoint(&pool, &job, token, &game, &mut result, None).await?;
    let statements: Vec<i64> =
        sqlx::query_scalar("SELECT rows FROM checkpoint_insert_counts ORDER BY id")
            .fetch_all(&pool)
            .await?;
    assert_eq!(statements, [500, 1], "one INSERT per bounded batch");

    let rows: Vec<(Option<Uuid>, i32, i32, String, f64, Value)> = sqlx::query_as(
        "SELECT position_uuid, decision_no, player, kind, equity_loss, data
         FROM move_analyses WHERE job_id=$1 ORDER BY id",
    )
    .bind(job.id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(rows.len(), game.decisions.len());
    for ((position, number, player, kind, loss, data), decision) in rows.iter().zip(&game.decisions)
    {
        assert_eq!(
            *position,
            (decision.kind == "checker").then_some(position_uuid)
        );
        assert_eq!(
            (*number, *player, kind, *loss),
            (
                decision.decision_no,
                decision.player,
                &decision.kind,
                decision.equity_loss,
            )
        );
        assert_eq!(*data, serde_json::to_value(decision)?);
    }
    let saved: (i32, i32, Value) =
        sqlx::query_as("SELECT completed_games, progress, result FROM jobs WHERE id=$1")
            .bind(job.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(saved, (1, 100, serde_json::to_value(&result)?));
    let game_summary: Value = sqlx::query_scalar(
        "SELECT summary FROM match_analysis_games WHERE job_id=$1 AND game_no=1",
    )
    .bind(job.id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(game_summary, saved.2);
    let progress: Value =
        sqlx::query_scalar("SELECT data FROM job_events WHERE job_id=$1 AND event='progress'")
            .bind(job.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        progress,
        json!({"status":"running","progress":100,"total_games":1,"completed_games":1})
    );

    let empty = AnalyzedGame {
        game_no: 2,
        decisions: Vec::new(),
        ..game
    };
    let (empty_job, empty_token) = running_job(&pool, match_id, empty.game_no).await?;
    let mut empty_result = summary(&empty);
    checkpoint(
        &pool,
        &empty_job,
        empty_token,
        &empty,
        &mut empty_result,
        None,
    )
    .await?;
    assert_eq!(empty_result.completed_games, 1);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM checkpoint_insert_counts")
            .fetch_one(&pool)
            .await?,
        2,
    );
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires GNUbg and pg_stat_statements preloaded on isolated PostgreSQL"]
async fn benchmark_game_checkpoint(pool: PgPool) -> Result<()> {
    let sgf = include_str!("../../tests/fixtures/WC2014.gnubg.sgf");
    let match_id = import_source(&pool, sgf).await?;
    let game_no: i32 = sqlx::query_scalar(
        "SELECT game_no FROM match_games WHERE match_uuid=$1
         ORDER BY position_count DESC, game_no LIMIT 1",
    )
    .bind(match_id)
    .fetch_one(&pool)
    .await?;
    let engine = Engine::new(std::env::var("GNUBG_BIN").unwrap_or_else(|_| "gnubg".into()));
    let game = engine.analyze_game(sgf, game_no, 1).await?;
    sqlx::query("CREATE EXTENSION pg_stat_statements")
        .execute(&pool)
        .await?;
    let stats = async || -> Result<(i64, i64, f64)> {
        Ok(sqlx::query_as(
            "SELECT coalesce(sum(calls),0)::bigint,coalesce(sum(rows),0)::bigint,
                    coalesce(sum(total_exec_time),0)
             FROM pg_stat_statements
             WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database())
               AND query LIKE 'INSERT INTO move_analyses%'",
        )
        .fetch_one(&pool)
        .await?)
    };
    const WARMUP: usize = 5;
    const SAMPLES: usize = 30;
    let mut durations = Vec::with_capacity(SAMPLES);
    let mut before = (0, 0, 0.0);
    for index in 0..WARMUP + SAMPLES {
        if index == WARMUP {
            before = stats().await?;
        }
        let (job, token) = running_job(&pool, match_id, game_no).await?;
        let mut result = summary(&game);
        let started = Instant::now();
        checkpoint(&pool, &job, token, &game, &mut result, None).await?;
        if index >= WARMUP {
            durations.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        assert_eq!(result.completed_games, 1);
        sqlx::query("UPDATE jobs SET status='completed' WHERE id=$1")
            .bind(job.id)
            .execute(&pool)
            .await?;
        if index == WARMUP {
            let digest: String = sqlx::query_scalar(
                "SELECT md5(string_agg(data::text, E'\\n' ORDER BY decision_no) ||
                             (SELECT result::text FROM jobs WHERE id=$1))
                 FROM move_analyses WHERE job_id=$1",
            )
            .bind(job.id)
            .fetch_one(&pool)
            .await?;
            println!("decisions and summary digest={digest}");
        }
    }
    let after = stats().await?;
    let (calls, rows, execution_ms) = (after.0 - before.0, after.1 - before.1, after.2 - before.2);
    assert_eq!(rows as usize, SAMPLES * game.decisions.len());
    durations.sort_by(f64::total_cmp);
    let median = (durations[SAMPLES / 2 - 1] + durations[SAMPLES / 2]) / 2.0;
    let p95 = durations[(SAMPLES * 95).div_ceil(100) - 1];
    println!(
        "{}; game {game_no}; 1-ply; {} checker moves, {} decisions; {WARMUP} warm-up, {SAMPLES} measured",
        game.engine,
        summary(&game).total_moves,
        game.decisions.len(),
    );
    println!(
        "checkpoint ms median={median:.3} p95={p95:.3} range={:.3}–{:.3}; decision INSERTs={calls} rows={rows} server execution ms/game={:.3}",
        durations[0],
        durations[SAMPLES - 1],
        execution_ms / SAMPLES as f64,
    );
    Ok(())
}
