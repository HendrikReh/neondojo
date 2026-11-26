//! Lease checks for exclusive benchmark and Engine Duel workers.
use anyhow::Result;
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::{Instant, timeout_at};
use uuid::Uuid;

pub(crate) const LEASE: Duration = Duration::from_secs(30);
const RENEW_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug)]
pub(crate) enum Owner {
    Benchmark,
    EngineDuel,
}

/// Return when the worker no longer owns a confirmed live lease. Keep polling
/// the measurement while the database is busy, up to the last known deadline.
pub(crate) async fn watch(
    owner: Owner,
    pool: &PgPool,
    id: Uuid,
    token: Uuid,
    mut renewed: Instant,
) -> Result<()> {
    let (table, token_column, interval) = match owner {
        Owner::Benchmark => ("benchmark_runs", "worker_token", Duration::from_millis(500)),
        Owner::EngineDuel => ("engine_match_runs", "lease_token", RENEW_INTERVAL),
    };
    let owned = format!("FROM {table} WHERE id=$1 AND {token_column}=$2 AND status='running'");
    let check = format!("SELECT EXISTS(SELECT 1 {owned} AND lease_until>now())");
    let renew = format!(
        "WITH owned AS MATERIALIZED (SELECT id,lease_until {owned} FOR UPDATE)
         UPDATE {table} SET lease_until=statement_timestamp()+make_interval(secs=>$3)
         FROM owned WHERE {table}.id=owned.id AND owned.lease_until>clock_timestamp()"
    );
    loop {
        let deadline = renewed + LEASE;
        let poll = async {
            tokio::time::sleep(interval).await;
            let mut db = pool.acquire().await?;
            let attempt = Instant::now();
            if attempt.duration_since(renewed) >= RENEW_INTERVAL {
                // Check expiry after acquiring the row lock. An UPDATE alone
                // can evaluate its predicate before waiting for another writer.
                let updated = sqlx::query(&renew)
                    .bind(id)
                    .bind(token)
                    .bind(LEASE.as_secs_f64())
                    .execute(&mut *db)
                    .await?;
                renewed = attempt;
                Ok(updated.rows_affected() == 1)
            } else {
                sqlx::query_scalar(&check)
                    .bind(id)
                    .bind(token)
                    .fetch_one(&mut *db)
                    .await
            }
        };
        match timeout_at(deadline, poll).await {
            Ok(Ok(true)) => {}
            Ok(Err(error)) if crate::jobs::transient(&error) => {
                tracing::warn!(%id, ?owner, %error, "Retrying worker lease check");
            }
            Ok(Ok(false)) | Err(_) => return Ok(()),
            Ok(Err(error)) => return Err(error.into()),
        }
    }
}
