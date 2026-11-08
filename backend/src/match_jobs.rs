//! Durable per-game checkpoints. All writes are fenced by the worker lease.
use crate::{api::AppState, match_analysis::AnalyzedGame, model::*};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub(crate) async fn execute(state: &AppState, job: &Job, token: Uuid) -> Result<Value> {
    let match_id = job.match_uuid.context("Match job has no match target")?;
    let source: String = sqlx::query_scalar("SELECT sgf FROM matches WHERE id=$1")
        .bind(match_id)
        .fetch_one(&state.pool)
        .await?;
    let total_moves: i32 = sqlx::query_scalar("SELECT position_count FROM matches WHERE id=$1")
        .bind(match_id)
        .fetch_one(&state.pool)
        .await?;
    let mut summary: MatchAnalysisSummary = if let Some(value) = &job.result {
        serde_json::from_value(value.clone()).context("Invalid saved match checkpoint")?
    } else {
        MatchAnalysisSummary {
            engine: state.engine.version().await?,
            plies: job.plies,
            total_games: job.total_games,
            completed_games: 0,
            total_moves,
            analyzed_moves: 0,
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
    ensure!(
        completed == (1..=job.completed_games).collect::<Vec<_>>()
            && summary.completed_games == job.completed_games,
        "Inconsistent match analysis checkpoint"
    );
    for game_no in job.completed_games + 1..=job.total_games {
        let analyzed = state
            .engine
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
        game.game_no == summary.completed_games + 1,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{engine::Engine, import, jobs, store};

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
}
