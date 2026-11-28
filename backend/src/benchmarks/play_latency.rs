use super::BenchmarkRun;
use crate::{api::AppState, engine::Engine, play::benchmark_checker_turn};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Instant;
use utoipa::ToSchema;
use uuid::Uuid;

// Standalone checkpoints built from the WC2014 reference before the recorded
// checker decision. Each stores the native GNUbg ID and the actual roll.
const TURNS: &str = include_str!("play_turns_v1.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PlayBenchmarkReference {
    pub id: String,
    pub sha256: String,
    pub total_turns: i32,
    pub protocol_id: String,
}

#[derive(Deserialize)]
struct Turn {
    game: i32,
    ply: i32,
    bot_player: i32,
    native_id: String,
    dice: [i32; 2],
    sgf: String,
}

pub(super) fn reference() -> Result<PlayBenchmarkReference> {
    Ok(PlayBenchmarkReference {
        id: "wc2014-checker-turns-v1".into(),
        sha256: format!("{:x}", Sha256::digest(TURNS.as_bytes())),
        total_turns: i32::try_from(serde_json::from_str::<Vec<Turn>>(TURNS)?.len())?,
        protocol_id: "gnubg-checker-turn-v2".into(),
    })
}

pub(super) fn reference_for(engine: &str, plies: i32) -> Result<PlayBenchmarkReference> {
    let mut value = reference()?;
    match crate::engines::EngineId::require(engine)? {
        crate::engines::EngineId::Gnubg => {}
        crate::engines::EngineId::NeonGammon => {
            value.protocol_id = format!("bgsage-checker-turn-v4-{plies}ply-2threads-gnubg-rules");
        }
    }
    Ok(value)
}

pub(super) async fn measure(state: &AppState, run: &BenchmarkRun, token: Uuid) -> Result<f64> {
    let pool = &state.pool;
    let expected = reference_for(&run.engine_id, run.plies)?;
    ensure!(
        run.play_reference.as_ref() == Some(&expected),
        "Unsupported play benchmark protocol or reference version"
    );
    let turns: Vec<Turn> = serde_json::from_str(TURNS)?;
    let engine = super::runner::saved_engine(pool, run).await?;
    let startup = super::runner::prepare(pool, run, token, &engine).await?;
    let saved = super::runner::saved_snapshot(pool, run.id).await?;
    let rules = if let Some(saved) = saved {
        let snapshot: crate::settings::EngineSnapshot =
            serde_json::from_value(saved["engine"].clone())?;
        Engine::new(snapshot.gnubg_path)
            .with_expected_version(saved["rules_version"].as_str().map(str::to_owned))
    } else {
        Engine::new(run.engine_path.clone()).with_expected_version(Some(run.engine_version.clone()))
    };
    engine
        .version()
        .await
        .context("Benchmark engine changed or is unavailable")?;
    super::runner::mark_started(pool, run.id, token).await?;
    let mut elapsed_ms = startup;
    for (index, turn) in turns.iter().enumerate() {
        let start = Instant::now();
        let result = match &engine {
            crate::engines::AnalysisEngine::Gnubg(engine) => {
                benchmark_checker_turn(
                    engine,
                    (&turn.sgf, &turn.native_id),
                    turn.bot_player,
                    turn.dice,
                    run.plies,
                )
                .await
            }
            crate::engines::AnalysisEngine::OpenSage(engine) => {
                crate::play::benchmark_sage_checker_turn(
                    engine,
                    &rules,
                    (&turn.sgf, &turn.native_id),
                    turn.bot_player,
                    turn.dice,
                )
                .await
            }
        };
        result.with_context(|| {
            format!(
                "Play benchmark game {}, checker turn {} failed",
                turn.game, turn.ply
            )
        })?;
        elapsed_ms += start.elapsed().as_secs_f64() * 1000.0;
        let updated = sqlx::query("UPDATE benchmark_runs SET completed_turns=$3 WHERE id=$1 AND worker_token=$2 AND status='running' AND lease_until>now()")
            .bind(run.id).bind(token).bind(i32::try_from(index + 1)?).execute(pool).await?;
        ensure!(updated.rows_affected() == 1, "Benchmark cancelled");
        state.notify_benchmark_update(run.id);
    }
    ensure!(
        elapsed_ms.is_finite() && elapsed_ms > 0.0,
        "Invalid elapsed time"
    );
    Ok(elapsed_ms)
}
