//! Durable per-game checkpoints. All writes are fenced by the worker lease.
use crate::{api::AppState, match_analysis::AnalyzedGame, model::*};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use std::time::Instant;
use uuid::Uuid;

#[cfg(test)]
mod checkpoint_tests;
#[cfg(test)]
mod tests;

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
    let (prior, remaining) = game_numbers
        .split_at_checked(usize::try_from(job.completed_games)?)
        .context("Invalid completed game count")?;
    ensure!(
        completed == prior && summary.completed_games == job.completed_games,
        "Inconsistent match analysis checkpoint"
    );
    crate::analysis_timing::initialize(
        &state.pool,
        job,
        token,
        engine,
        engine.saved_version().unwrap_or(&summary.engine),
        total_moves,
        summary.analyzed_moves,
    )
    .await?;
    if !remaining.is_empty() {
        let prepared = crate::engines::replay::MatchEvaluator::prepare(engine, source).await?;
        for &game_no in remaining {
            crate::engine::with_shared_access(async {
                crate::analysis_timing::start_game(&state.pool, job, token).await?;
                let started = Instant::now();
                let work = async {
                    let analyzed = prepared
                        .analyze_game(game_no, job.plies, Some(crate::engines::GAME_TIMEOUT))
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
                let samples = crate::analysis_timing::sample_until_lease_lost(
                    &state.pool,
                    job,
                    token,
                    started,
                    std::time::Duration::from_secs(1),
                );
                tokio::select! {
                    result = work => result,
                    result = samples => result,
                }
            })
            .await?;
        }
    }
    ensure!(
        summary.analyzed_moves == total_moves,
        "{} analyzed {} of {total_moves} checker moves",
        engine.id().display_name(),
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
    let id = job
        .engine_snapshot
        .as_ref()
        .map_or("gnubg", |s| s.engine_id.as_str());
    let provider = crate::engines::EngineId::require(id)?.display_name();
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
        "Game {}: {provider} returned {} of {} checker decisions",
        game.game_no,
        checkers.len(),
        positions.len()
    );
    for (index, (decision, position)) in checkers.iter().zip(&positions).enumerate() {
        ensure!(
            decision.checker_no == Some(index as i32 + 1)
                && decision.player == position.player
                && decision.dice == Some([position.dice[0], position.dice[1]]),
            "Game {}: {provider} checker decision {} does not match the imported game",
            game.game_no,
            index + 1
        );
    }
    let mut next = summary.clone();
    accumulate_performance_rating(&mut next, game);
    next.engine = match &job.engine_snapshot {
        Some(snapshot) if summary.completed_games > 0 && summary.engine != game.engine => {
            snapshot.open_sage.as_ref().map_or_else(
                || game.engine.clone(),
                |sage| sage.describe("source cube rules"),
            )
        }
        _ => game.engine.clone(),
    };
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
                "Invalid {provider} decision order/player"
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
    let mut game_summary = serde_json::to_value(game_summary)?;
    if !game.cube_enabled {
        game_summary["cube_enabled"] = Value::Bool(false);
    }
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
    sqlx::query("UPDATE jobs SET completed_games=$3,progress=$4,result=$5,updated_at=now(),lease_until=now()+make_interval(secs=>$6) WHERE id=$1 AND lease_token=$2")
        .bind(job.id)
        .bind(token)
        .bind(next.completed_games)
        .bind(progress)
        .bind(result)
        .bind(crate::jobs::LEASE.as_secs_f64())
        .execute(&mut *tx)
        .await?;
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
    let cubeful = crate::match_analysis::performance_rating_profile(&game.engine, summary.plies);
    let source_rules =
        crate::match_analysis::rule_aware_rating_profile(&game.engine, summary.plies);
    if summary.completed_games == 0 {
        summary.performance_rating = Some(PerformanceRatingSummary {
            method: crate::match_analysis::PERFORMANCE_RATING_METHOD.into(),
            profile: cubeful.clone(),
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
        || (rating.profile != cubeful && rating.profile != source_rules)
    {
        summary.performance_rating = None;
        return;
    }
    if !game.cube_enabled {
        rating.profile = source_rules;
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
