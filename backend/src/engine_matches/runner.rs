use super::{Competitor, DiceMode, Series, SeriesConfig, dice};
use crate::{
    api::AppState,
    engine::Engine,
    import::{parse_sgf, sgf::escape_value},
    play::engine_match::{self, DuelUnit, GameDice, SideCost},
    store::{MatchImport, insert_match_import},
};
use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

pub(crate) const ENGINE_DUELS_DATABASE_ID: Uuid = Uuid::from_u128(5);

pub async fn worker(state: AppState) {
    loop {
        tokio::select! {
            _ = state.shutdown.cancelled() => break,
            result = async {
                let Ok(_access) = state.backup.requests.try_read() else {
                    return Ok(false);
                };
                run_one(&state.pool).await
            } => match result {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => tracing::error!(%error, "Engine match worker error"),
            },
        }
        tokio::select! {
            _ = state.shutdown.cancelled() => break,
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
    }
}

pub(super) async fn run_one(pool: &PgPool) -> Result<bool> {
    let token = Uuid::new_v4();
    let Some(series) = claim(pool, token).await? else {
        return Ok(false);
    };
    let result = tokio::select! {
        result = execute(pool, &series, token) => result,
        result = monitor(pool, series.id, token) => result,
    };
    if let Err(error) = result {
        sqlx::query("UPDATE engine_match_runs SET status='failed',error=$3,lease_token=NULL,lease_until=NULL WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
            .bind(series.id).bind(token).bind(format!("{error:#}")).execute(pool).await?;
    }
    Ok(true)
}

async fn claim(pool: &PgPool, token: Uuid) -> Result<Option<Series>> {
    let mut tx = pool.begin().await?;
    let owns: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(91432302)")
        .fetch_one(&mut *tx)
        .await?;
    if !owns {
        return Ok(None);
    }
    sqlx::query("UPDATE engine_match_runs SET status=CASE WHEN attempts>=3 THEN 'failed' ELSE 'queued' END,error=CASE WHEN attempts>=3 THEN 'Worker lease expired after three attempts' ELSE NULL END,lease_token=NULL,lease_until=NULL WHERE status='running' AND lease_until<=now()")
        .execute(&mut *tx).await?;
    let series = sqlx::query_as("UPDATE engine_match_runs SET status='running',lease_token=$1,lease_until=now()+interval '30 seconds',attempts=attempts+1 WHERE id=(SELECT id FROM engine_match_runs WHERE status='queued' AND NOT EXISTS(SELECT 1 FROM engine_match_runs WHERE status='running') ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *")
        .bind(token).fetch_optional(&mut *tx).await?;
    tx.commit().await?;
    Ok(series)
}

async fn monitor(pool: &PgPool, id: Uuid, token: Uuid) -> Result<()> {
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let updated = sqlx::query("UPDATE engine_match_runs SET lease_until=now()+interval '30 seconds' WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
            .bind(id).bind(token).execute(pool).await?;
        ensure!(
            updated.rows_affected() == 1,
            "Engine match cancelled or worker lease lost"
        );
    }
}

async fn execute(pool: &PgPool, series: &Series, token: Uuid) -> Result<()> {
    ensure!(
        series.dice_version == dice::VERSION,
        "Unsupported saved dice version"
    );
    let rules = Engine::new(series.engines.a.gnubg_path.clone())
        .with_threads(
            series
                .engines
                .a
                .gnubg_threads
                .or(series.engines.b.gnubg_threads),
        )
        .with_expected_version(Some(series.engines.rules_version.clone()));
    let a = series.engines.a.engine()?;
    let b = series.engines.b.engine()?;
    let configuration = &series.configuration;
    let unit = configuration.unit()?;
    let event = escape_value(&format!(
        "Duel {} vs. {}{}",
        competitor_label(&configuration.a)?,
        competitor_label(&configuration.b)?,
        match unit {
            DuelUnit::Match { .. } => "",
            DuelUnit::MoneyCubeless { .. } => " (money, cubeless)",
        },
    ));
    for index in series.completed_matches..series.total_matches {
        let plan = match_plan(configuration, index);
        let mut bots = [&a, &b];
        let mut depths = [configuration.a.plies, configuration.b.plies];
        if plan.swapped {
            bots.swap(0, 1);
            depths.swap(0, 1);
        }
        let finished = engine_match::play(&rules, bots, depths, unit, |game| {
            Ok(GameDice {
                opening: dice::opening(plan.seed, game)?,
                turns: dice::turns(plan.seed, game),
            })
        })
        .await?;
        let mut scores = finished.scores;
        let mut costs = finished.costs;
        let mut outcomes = finished.outcomes;
        if plan.swapped {
            scores.swap(0, 1);
            costs.swap(0, 1);
            if let Some(outcomes) = outcomes.as_mut() {
                outcomes.swap(0, 1);
            }
        }
        let [cost_a, cost_b] = costs.map(|side| side.map(stored_cost).transpose());
        let (cost_a, cost_b) = (cost_a?, cost_b?);
        match unit {
            DuelUnit::Match { length } => ensure!(
                (scores[0] >= length) ^ (scores[1] >= length) && outcomes.is_none(),
                "Missing native match winner"
            ),
            DuelUnit::MoneyCubeless { games } => {
                let counts = outcomes.context("Missing cubeless money game results")?;
                let points = |side: [i32; 3]| side[0] + 2 * side[1] + 3 * side[2];
                ensure!(
                    finished.games == games
                        && counts.iter().flatten().sum::<i32>() == games
                        && scores == counts.map(points),
                    "Inconsistent cubeless money block result"
                );
            }
        }
        // Stored as A single, gammon, backgammon, then B; only for money blocks.
        let stored_outcomes = outcomes.map(|counts| counts.concat());
        let a_name = format!(
            "A {} {}-ply",
            configuration.a.engine_id, configuration.a.plies
        );
        let b_name = format!(
            "B {} {}-ply",
            configuration.b.engine_id, configuration.b.plies
        );
        let mut sgf =
            crate::play::named_export(&finished.sgf, &a_name, i32::from(plan.swapped), &b_name);
        // Each run owns its played matches, even if a seed replay produces identical moves.
        let root = sgf.find(';').context("Missing engine duel SGF root")?;
        sgf.insert_str(
            root + 1,
            &format!("BGTDUEL[{}:{index}]EV[{event}]", series.id),
        );
        let (sgf, imported) = tokio::task::spawn_blocking(move || {
            let imported = parse_sgf(&sgf).context("Invalid engine duel SGF")?;
            Ok::<_, anyhow::Error>((sgf, imported))
        })
        .await??;
        let filename = format!("Engine duel {} #{}.sgf", series.id, index + 1);
        let archive =
            MatchImport::prepare(&filename, "sgf", sgf.as_bytes(), &sgf, &imported).await?;
        let mut tx = pool.begin().await?;
        insert_match_import(&mut tx, ENGINE_DUELS_DATABASE_ID, archive).await?;
        let updated = sqlx::query("UPDATE engine_match_runs SET completed_matches=$3+1, attempts=1, status=CASE WHEN $3+1=total_matches THEN 'completed' ELSE 'running' END,lease_token=CASE WHEN $3+1=total_matches THEN NULL ELSE lease_token END,lease_until=CASE WHEN $3+1=total_matches THEN NULL ELSE lease_until END WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>clock_timestamp() AND completed_matches=$3")
            .bind(series.id).bind(token).bind(index).execute(&mut *tx).await?;
        ensure!(
            updated.rows_affected() == 1,
            "Engine match checkpoint lost its lease"
        );
        sqlx::query("INSERT INTO engine_match_results(run_id,match_index,seed,score_a,score_b,games,sgf,cpu_us_a,wall_us_a,moves_a,cpu_us_b,wall_us_b,moves_b,outcomes) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
            .bind(series.id).bind(index).bind(plan.seed).bind(scores[0]).bind(scores[1])
            .bind(finished.games).bind(sgf)
            .bind(cost_a.map(|cost| cost.0)).bind(cost_a.map(|cost| cost.1)).bind(cost_a.map(|cost| cost.2))
            .bind(cost_b.map(|cost| cost.0)).bind(cost_b.map(|cost| cost.1)).bind(cost_b.map(|cost| cost.2))
            .bind(stored_outcomes)
            .execute(&mut *tx).await?;
        tx.commit().await?;
    }
    Ok(())
}

/// Database form of one side's cost: CPU µs, wall µs and checker turns.
fn stored_cost(side: SideCost) -> Result<(i64, i64, i32)> {
    Ok((
        i64::try_from(side.cost.cpu_us).context("Engine CPU time exceeds storage range")?,
        i64::try_from(side.cost.wall_us).context("Engine wall time exceeds storage range")?,
        side.moves,
    ))
}

fn competitor_label(side: &Competitor) -> Result<String> {
    let name = match side.engine_id.as_str() {
        "gnubg" => "GNUbg",
        "bgsage" => "Open Sage",
        _ => bail!("Unsupported saved duel engine {}", side.engine_id),
    };
    Ok(format!("{name} {}-ply", side.plies))
}

pub(super) struct MatchPlan {
    pub seed: Uuid,
    pub swapped: bool,
}

pub(super) fn match_plan(config: &SeriesConfig, index: i32) -> MatchPlan {
    let mut hash = Sha256::new();
    hash.update(dice::VERSION);
    hash.update(b"/unit/");
    hash.update(config.seed.as_bytes());
    hash.update((index / config.mode.matches_per_unit()).to_be_bytes());
    let digest: [u8; 32] = hash.finalize().into();
    let (bytes, _) = digest.split_at(16);
    let mut seed = [0; 16];
    seed.copy_from_slice(bytes);
    MatchPlan {
        seed: Uuid::from_bytes(seed),
        swapped: config.mode == DiceMode::Paired && index % 2 == 1,
    }
}
