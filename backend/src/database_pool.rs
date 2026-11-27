use sqlx::{Executor, PgPool, Postgres, Transaction, postgres::PgPoolOptions};
use std::time::Duration;

/// Opens a `REPEATABLE READ READ ONLY` snapshot transaction in a single round
/// trip so multi-query browse/review reads observe a consistent database state.
pub(crate) async fn begin_read_snapshot(
    pool: &PgPool,
) -> anyhow::Result<Transaction<'static, Postgres>> {
    Ok(pool
        .begin_with("BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .await?)
}

/// Connections when `NEONDOJO_DB_MAX_CONNECTIONS` is unset. Up to eight
/// parallel analysis jobs each sample progress, renew a lease and write
/// checkpoints while the API, background workers and backups also query.
pub(crate) const DEFAULT_MAX_CONNECTIONS: u32 = 32;
/// How long a query waits for a free connection before it fails.
pub(crate) const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(15);

/// Pool size from the optional `NEONDOJO_DB_MAX_CONNECTIONS` value.
pub(crate) fn max_connections(configured: Option<&str>) -> anyhow::Result<u32> {
    let Some(configured) = configured else {
        return Ok(DEFAULT_MAX_CONNECTIONS);
    };
    match configured.trim().parse::<u32>() {
        Ok(connections @ 1..=1000) => Ok(connections),
        _ => anyhow::bail!("NEONDOJO_DB_MAX_CONNECTIONS must be a whole number from 1 to 1000"),
    }
}

pub(crate) fn options() -> PgPoolOptions {
    PgPoolOptions::new()
        // List queries join many small per-row lookups. Their estimated cost
        // crosses PostgreSQL's JIT threshold although they touch few rows, and
        // compiling their expressions then takes longer than running them.
        .after_connect(|connection, _| {
            Box::pin(async move {
                connection.execute("SET jit = off").await?;
                Ok(())
            })
        })
        // SQLx 0.8 can return a live connection after BEGIN reached PostgreSQL
        // but the caller stopped waiting before SQLx recorded the transaction.
        // Use a PostgreSQL check in place of the normal checkout ping. A DO
        // block can end its own transaction only when run outside an explicit
        // transaction. An open or failed transaction makes the check error;
        // SQLx then discards the connection before handing it to a borrower.
        .test_before_acquire(false)
        .before_acquire(|connection, _| {
            Box::pin(async move {
                connection.execute("DO $$ BEGIN ROLLBACK; END $$").await?;
                Ok(true)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[test]
    fn pool_size_defaults_and_rejects_invalid_overrides() {
        assert_eq!(max_connections(None).unwrap(), DEFAULT_MAX_CONNECTIONS);
        assert_eq!(max_connections(Some(" 48 ")).unwrap(), 48);
        for invalid in ["", "0", "-1", "ten", "1001"] {
            assert!(max_connections(Some(invalid)).is_err(), "{invalid:?}");
        }
    }

    #[sqlx::test(migrations = false)]
    async fn begin_read_snapshot_opens_repeatable_read_only_transaction(db: PgPool) {
        let mut tx = begin_read_snapshot(&db).await.unwrap();
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let read_only: String = sqlx::query_scalar("SHOW transaction_read_only")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(isolation, "repeatable read");
        assert_eq!(read_only, "on");
        tx.commit().await.unwrap();
    }

    #[sqlx::test(migrations = false)]
    async fn cancelled_begin_cannot_leak_a_transaction_to_the_next_borrower(db: PgPool) {
        let pool = options()
            .max_connections(1)
            .connect_with((*db.connect_options()).clone())
            .await
            .unwrap();
        let original_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&pool)
            .await
            .unwrap();

        // Hold BEGIN's response until PostgreSQL has entered the transaction.
        // The transaction-scoped lock also disappears when the blocker is dropped.
        let mut blocker = db.begin().await.unwrap();
        sqlx::query("SELECT pg_advisory_xact_lock(736)")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let opener = tokio::spawn({
            let pool = pool.clone();
            async move {
                pool.begin_with("BEGIN READ ONLY; SELECT pg_advisory_xact_lock(736)")
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let waiting: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM pg_stat_activity \
                     WHERE pid = $1 AND wait_event_type = 'Lock' AND wait_event = 'advisory')",
                )
                .bind(original_pid)
                .fetch_one(&db)
                .await
                .unwrap();
                if waiting {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("PostgreSQL should be waiting inside BEGIN");
        opener.abort();
        assert!(opener.await.unwrap_err().is_cancelled());
        blocker.rollback().await.unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let mode: String = sqlx::query_scalar("SHOW transaction_read_only")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(
            mode, "off",
            "the next borrower inherited the read-only transaction"
        );
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        drop(conn);
        let next_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(pid, next_pid, "a clean connection should be reused");
        pool.close().await;
    }
}
