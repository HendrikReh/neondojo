use crate::{
    api::AppState,
    engine::{DiceEvaluation, Distribution},
    model::Job,
    store,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub async fn worker(state: AppState, stop: CancellationToken) {
    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            result = run_one(&state) => {
                match result {
                    Ok(true) => continue,
                    Ok(false) => {},
                    Err(error) => tracing::error!(%error, "Job worker error"),
                }
            }
        }
        tokio::select! {
            _ = stop.cancelled() => break,
            _ = state.wake.notified() => {},
            _ = tokio::time::sleep(Duration::from_secs(1)) => {},
        }
    }
}

pub async fn run_one(state: &AppState) -> Result<bool> {
    let token = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    let expired: Vec<Uuid> = sqlx::query_scalar("UPDATE jobs SET status=CASE WHEN attempts>=3 THEN 'failed' ELSE 'queued' END, error=CASE WHEN attempts>=3 THEN 'Worker lease expired after 3 attempts' ELSE NULL END, lease_token=NULL, lease_until=NULL, progress=CASE WHEN kind='match' THEN progress ELSE 0 END, updated_at=now() WHERE status='running' AND lease_until<now() RETURNING id")
        .fetch_all(&mut *tx).await?;
    for id in expired {
        sqlx::query("INSERT INTO job_events(job_id,event,data) SELECT id,status,jsonb_build_object('status',status,'progress',progress,'error',error,'completed_games',completed_games,'total_games',total_games) FROM jobs WHERE id=$1")
            .bind(id).execute(&mut *tx).await?;
    }
    let job = sqlx::query_as::<_, Job>("UPDATE jobs SET status='running',lease_token=$1,lease_until=now()+interval '120 seconds',attempts=attempts+1,updated_at=now() WHERE id=(SELECT id FROM jobs WHERE status='queued' ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *")
        .bind(token).fetch_optional(&mut *tx).await?;
    if let Some(ref job) = job {
        sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'running',$2)")
            .bind(job.id)
            .bind(json!({"status":"running","progress":job.progress,"attempt":job.attempts,"completed_games":job.completed_games,"total_games":job.total_games}))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    let Some(job) = job else { return Ok(false) };
    let result = execute_with_heartbeat(state, &job, token).await;
    let (status, data, error) = match result {
        Ok(data) => ("completed", Some(data), None),
        Err(error) => ("failed", None, Some(format!("{error:#}"))),
    };
    let mut tx = state.pool.begin().await?;
    let changed = sqlx::query("UPDATE jobs SET status=$3,result=COALESCE($4,result),error=$5,progress=CASE WHEN $3='completed' THEN 100 ELSE progress END,lease_token=NULL,lease_until=NULL,updated_at=now() WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
        .bind(job.id).bind(token).bind(status).bind(&data).bind(&error).execute(&mut *tx).await?;
    if changed.rows_affected() == 1 {
        sqlx::query("INSERT INTO job_events(job_id,event,data) SELECT id,status,jsonb_build_object('status',status,'progress',progress,'error',error,'completed_games',completed_games,'total_games',total_games) FROM jobs WHERE id=$1")
            .bind(job.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(true)
}

async fn execute_with_heartbeat(state: &AppState, job: &Job, token: Uuid) -> Result<Value> {
    with_heartbeat(
        &state.pool,
        job.id,
        token,
        execute(state, job, token),
        Duration::from_secs(20),
    )
    .await
}

async fn with_heartbeat(
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    execution: impl std::future::Future<Output = Result<Value>>,
    interval: Duration,
) -> Result<Value> {
    // Keep polling the work while renewal waits for its row lock. A checkpoint
    // can own that lock until its transaction commits; awaiting renewal inside
    // an execution-select branch would stop polling the commit and deadlock.
    let heartbeat = async {
        loop {
            tokio::time::sleep(interval).await;
            let updated = sqlx::query("UPDATE jobs SET lease_until=now()+interval '120 seconds' WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
                .bind(id).bind(token).execute(pool).await?;
            ensure!(updated.rows_affected() == 1, "Job lease was lost");
        }
    };
    tokio::select! {
        result = execution => result,
        result = heartbeat => result,
    }
}

async fn execute(state: &AppState, job: &Job, token: Uuid) -> Result<Value> {
    let configured_engine = store::job_engine(&state.pool, job.id, &state.engine).await?;
    if job.kind == "match" {
        return crate::match_jobs::execute(state, job, token, &configured_engine).await;
    }
    let position = store::get_position(
        &state.pool,
        job.position_uuid
            .context("Position job has no position target")?,
    )
    .await?
    .context("Position no longer exists")?;
    if job.kind == "analysis" {
        return Ok(serde_json::to_value(
            configured_engine
                .analyze(
                    &position.position_id,
                    &position.gnubg_match_id,
                    job.plies,
                    None,
                )
                .await?,
        )?);
    }
    let mut rolls = Vec::new();
    let mut engine = String::new();
    for a in 1..=6 {
        for b in a..=6 {
            let analysis = configured_engine
                .analyze(
                    &position.position_id,
                    &position.gnubg_match_id,
                    job.plies,
                    Some([a, b]),
                )
                .await?;
            engine = analysis.engine;
            let best = analysis
                .candidates
                .into_iter()
                .next()
                .context("No evaluated candidate")?;
            let weight = if a == b { 1 } else { 2 };
            rolls.push(DiceEvaluation {
                dice: [a, b],
                weight,
                probability: f64::from(weight) / 36.0,
                best,
            });
            progress(&state.pool, job.id, token, (rolls.len() * 100 / 21) as i32).await?;
        }
    }
    let mean_equity = rolls.iter().map(|r| r.best.equity * r.probability).sum();
    Ok(serde_json::to_value(Distribution {
        engine,
        plies: job.plies,
        mean_equity,
        rolls,
    })?)
}

async fn progress(pool: &PgPool, id: Uuid, token: Uuid, progress: i32) -> Result<()> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query("UPDATE jobs SET progress=$3,updated_at=now(),lease_until=now()+interval '120 seconds' WHERE id=$1 AND lease_token=$2 AND status='running' AND lease_until>now()")
        .bind(id).bind(token).bind(progress).execute(&mut *tx).await?;
    ensure!(result.rows_affected() == 1, "Job lease was lost");
    sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'progress',$2)")
        .bind(id)
        .bind(json!({"status":"running","progress":progress}))
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires PostgreSQL"]
    async fn heartbeat_does_not_deadlock_a_checkpoint_holding_the_job_lock(pool: PgPool) {
        let sgf = "(;GM[6]FF[4]PW[Alice]PB[Bob]MI[length:7][game:0][ws:0][bs:0];W[31hefe])";
        let imported = store::import_match(
            &pool,
            "lock.sgf",
            "sgf",
            sgf.as_bytes(),
            sgf,
            &crate::import::parse_sgf(sgf).unwrap(),
        )
        .await
        .unwrap();
        let job = store::create_match_job(&pool, imported.r#match.id, 0, 1)
            .await
            .unwrap();
        let token = Uuid::new_v4();
        sqlx::query("UPDATE jobs SET status='running',lease_token=$2,lease_until=now()+interval '120 seconds' WHERE id=$1").bind(job.id).bind(token).execute(&pool).await.unwrap();
        let mut checkpoint = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM jobs WHERE id=$1 FOR UPDATE")
            .bind(job.id)
            .execute(&mut *checkpoint)
            .await
            .unwrap();
        let execution = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            checkpoint.commit().await?;
            Ok(json!({"checkpoint":"committed"}))
        };
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            with_heartbeat(&pool, job.id, token, execution, Duration::from_millis(10)),
        )
        .await
        .expect("heartbeat must not stop polling the checkpoint while waiting for its row lock")
        .unwrap();
        assert_eq!(result["checkpoint"], "committed");
    }
}
