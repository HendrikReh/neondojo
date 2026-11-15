use sqlx::{Executor, postgres::PgPoolOptions};

pub(crate) fn options() -> PgPoolOptions {
    PgPoolOptions::new()
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
    use std::time::Duration;

    #[sqlx::test(migrations = false)]
    #[ignore = "requires PostgreSQL"]
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
