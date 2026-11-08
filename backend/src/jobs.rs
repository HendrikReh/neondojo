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
    let expired: Vec<Uuid> = sqlx::query_scalar("UPDATE jobs SET status=CASE WHEN attempts>=3 THEN 'failed' ELSE 'queued' END, error=CASE WHEN attempts>=3 THEN 'Worker lease expired after 3 attempts' ELSE NULL END, lease_token=NULL, lease_until=NULL, progress=0, updated_at=now() WHERE status='running' AND lease_until<now() RETURNING id")
        .fetch_all(&mut *tx).await?;
    for id in expired {
        sqlx::query("INSERT INTO job_events(job_id,event,data) SELECT id,status,jsonb_build_object('status',status,'progress',progress,'error',error) FROM jobs WHERE id=$1")
            .bind(id).execute(&mut *tx).await?;
    }
    let job = sqlx::query_as::<_, Job>("UPDATE jobs SET status='running',lease_token=$1,lease_until=now()+interval '120 seconds',attempts=attempts+1,updated_at=now() WHERE id=(SELECT id FROM jobs WHERE status='queued' ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *")
        .bind(token).fetch_optional(&mut *tx).await?;
    if let Some(ref job) = job {
        sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,'running',$2)")
            .bind(job.id)
            .bind(json!({"status":"running","progress":0,"attempt":job.attempts}))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    let Some(job) = job else { return Ok(false) };
    let result = execute(state, &job, token).await;
    let (status, data, error) = match result {
        Ok(data) => ("completed", Some(data), None),
        Err(error) => ("failed", None, Some(format!("{error:#}"))),
    };
    let mut tx = state.pool.begin().await?;
    let changed = sqlx::query("UPDATE jobs SET status=$3,result=$4,error=$5,progress=CASE WHEN $3='completed' THEN 100 ELSE progress END,lease_token=NULL,lease_until=NULL,updated_at=now() WHERE id=$1 AND lease_token=$2 AND status='running'")
        .bind(job.id).bind(token).bind(status).bind(&data).bind(&error).execute(&mut *tx).await?;
    if changed.rows_affected() == 1 {
        sqlx::query("INSERT INTO job_events(job_id,event,data) SELECT id,status,jsonb_build_object('status',status,'progress',progress,'error',error) FROM jobs WHERE id=$1")
            .bind(job.id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(true)
}

async fn execute(state: &AppState, job: &Job, token: Uuid) -> Result<Value> {
    let position = store::get_position(&state.pool, job.position_uuid)
        .await?
        .context("Position no longer exists")?;
    if job.kind == "analysis" {
        return Ok(serde_json::to_value(
            state
                .engine
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
            let analysis = state
                .engine
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
    let result = sqlx::query("UPDATE jobs SET progress=$3,updated_at=now(),lease_until=now()+interval '120 seconds' WHERE id=$1 AND lease_token=$2 AND status='running'")
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
