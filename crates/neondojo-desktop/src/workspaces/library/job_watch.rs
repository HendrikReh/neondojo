//! Follow the committed state behind visible Library rows and accepted bulk jobs.
//! Both clients share PostgreSQL's job-event listener. The Library displays the
//! current status, so reconnecting needs only a fresh snapshot of its scope.

use super::LibraryWorkspace;
use crate::tokio_bridge::{ReadTask, TokioBridge};
use gpui_kit::{Context, Task};
use neondojo::{databases::LibraryBulkJobStatus, domain_error::DomainError, facade::BackendFacade};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

#[derive(PartialEq, Eq)]
struct WatchScope {
    rows: Vec<Uuid>,
    bulk: Vec<Uuid>,
    pending: bool,
}

pub(super) struct LibraryJobWatch {
    scope: Arc<WatchScope>,
    _reader: ReadTask<()>,
    _delivery: Task<()>,
}

struct Snapshot {
    rows: Vec<(Uuid, Option<String>)>,
    missing_row: bool,
    bulk: Vec<LibraryBulkJobStatus>,
}

impl WatchScope {
    async fn snapshot(
        &self,
        facade: &BackendFacade,
        changed_job: Option<Uuid>,
    ) -> Result<Snapshot, DomainError> {
        let rows = if self.rows.is_empty() {
            Vec::new()
        } else {
            facade
                .library_analysis_statuses(&self.rows, changed_job)
                .await?
        };
        let missing_row = changed_job.is_none() && rows.len() < self.rows.len();
        let ids = match changed_job.as_ref() {
            None => &self.bulk,
            Some(id) if self.bulk.contains(id) => std::slice::from_ref(id),
            Some(_) => &[][..],
        };
        let bulk = if ids.is_empty() {
            Vec::new()
        } else {
            facade.library_bulk_job_statuses(ids).await?
        };
        Ok(Snapshot {
            rows,
            missing_row,
            bulk,
        })
    }

    async fn follow(self: Arc<Self>, facade: BackendFacade, tx: mpsc::Sender<Snapshot>) {
        // Subscribe before reading so commits during a query are queued.
        let mut hints = facade.subscribe_job_updates();
        let mut changed_job = None;
        let recheck = Duration::from_secs(15);
        let mut safety_read =
            tokio::time::interval_at(tokio::time::Instant::now() + recheck, recheck);
        safety_read.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            // Multiple commits, lag or listener reconnect require one full reconciliation.
            if !hints.is_empty() {
                hints = hints.resubscribe();
                changed_job = None;
            }
            let result = self.snapshot(&facade, changed_job).await;
            let retry = result.is_err();
            match result {
                Ok(snapshot) => {
                    if tx.send(snapshot).await.is_err() {
                        return;
                    }
                }
                Err(err) => tracing::warn!(%err, "Could not read Library job status"),
            }
            let hint = tokio::select! {
                _ = safety_read.tick(), if retry || self.pending || !self.bulk.is_empty() => Ok(None),
                hint = hints.recv() => hint,
            };
            changed_job = match hint {
                Ok(_) if retry => None,
                Ok(job) => job,
                Err(broadcast::error::RecvError::Lagged(_)) => None,
                Err(broadcast::error::RecvError::Closed) => return,
            };
        }
    }
}

impl LibraryWorkspace {
    pub(super) fn sync_job_watcher(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = self.facade.clone().filter(|_| self.visible) else {
            self.job_watch = None;
            return;
        };
        let scope = Arc::new(WatchScope {
            rows: self
                .matches
                .iter()
                .map(|m| m.id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            bulk: self.bulk_tracker.active_job_ids(),
            pending: self
                .matches
                .iter()
                .any(|m| matches!(m.analysis_status.as_deref(), Some("queued" | "running"))),
        });
        if scope.rows.is_empty() && scope.bulk.is_empty() {
            self.job_watch = None;
            return;
        }
        if self
            .job_watch
            .as_ref()
            .is_some_and(|watch| watch.scope == scope)
        {
            return;
        }

        let (tx, mut rx) = mpsc::channel(1);
        let watched = scope.clone();
        let reader = TokioBridge::spawn_read(cx, watched.follow(facade, tx));
        let delivery_scope = scope.clone();
        let delivery = cx.spawn(async move |this, cx| {
            while let Some(snapshot) = rx.recv().await {
                if this
                    .update(cx, |ws, cx| {
                        if ws
                            .job_watch
                            .as_ref()
                            .is_some_and(|w| Arc::ptr_eq(&w.scope, &delivery_scope))
                        {
                            ws.apply_job_snapshot(snapshot, cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        self.job_watch = Some(LibraryJobWatch {
            scope,
            _reader: reader,
            _delivery: delivery,
        });
    }

    fn apply_job_snapshot(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        let mut reload = snapshot.missing_row
            || snapshot.rows.iter().any(|(id, status)| {
                self.matches
                    .iter()
                    .any(|m| m.id == *id && m.analysis_status != *status)
            });
        let (changed, finished) = self.bulk_tracker.apply_batch(snapshot.bulk);
        if finished {
            reload = true;
            self.sync_job_watcher(cx);
        }
        if reload {
            if self.loading_matches {
                // Let the in-flight query finish, then cover any later commit.
                self.pending_job_refresh = true;
            } else {
                self.load_matches(cx);
            }
        }
        if changed {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neondojo::{api::AppState, engine::Engine, store};
    use sqlx::PgPool;

    // Each expected transition must arrive before the 15 s safety read.
    async fn expect_snapshot(
        rx: &mut mpsc::Receiver<Snapshot>,
        match_id: Uuid,
        job_id: Uuid,
        status: &str,
        progress: i32,
    ) {
        tokio::time::timeout(Duration::from_secs(6), async {
            while let Some(snapshot) = rx.recv().await {
                if snapshot
                    .bulk
                    .iter()
                    .any(|job| job.id == job_id && job.status == status && job.progress == progress)
                {
                    assert_eq!(snapshot.rows, vec![(match_id, Some(status.into()))]);
                    return;
                }
            }
            panic!("Library watcher ended before {status} {progress}");
        })
        .await
        .unwrap_or_else(|_| panic!("Library stayed stale at {status} {progress}"));
    }

    #[sqlx::test(migrations = "../../backend/migrations")]
    #[ignore = "requires a local PostgreSQL test server for the macOS desktop crate"]
    async fn row_and_bulk_follow_commits_and_reconcile_after_listener_reconnect(pool: PgPool) {
        let sgf = "(;GM[6]FF[4]MI[length:5][ws:0][bs:0];W[31hefe])";
        let imported = store::import_match(
            &pool,
            "watch.sgf",
            "sgf",
            sgf.as_bytes(),
            sgf,
            &neondojo::import::parse_sgf(sgf).unwrap(),
        )
        .await
        .unwrap();
        let match_id = imported.r#match.id;
        let job = store::create_match_job(&pool, match_id, 1, 1)
            .await
            .unwrap();
        let facade = BackendFacade::new(AppState::new(pool.clone(), Engine::new("unused".into())));
        let (tx, mut rx) = mpsc::channel(1);
        let watch = WatchScope {
            rows: vec![match_id],
            bulk: vec![job.id],
            pending: true,
        };
        let reader = tokio::spawn(Arc::new(watch).follow(facade.clone(), tx));
        expect_snapshot(&mut rx, match_id, job.id, "queued", 0).await;

        for (event, status, progress) in [
            ("running", "running", 0),
            ("progress", "running", 40),
            ("completed", "completed", 100),
        ] {
            let mut tx = pool.begin().await.unwrap();
            sqlx::query("UPDATE jobs SET status=$2, progress=$3 WHERE id=$1")
                .bind(job.id)
                .bind(status)
                .bind(progress)
                .execute(&mut *tx)
                .await
                .unwrap();
            if status == "completed" {
                sqlx::query("UPDATE jobs SET completed_games=total_games WHERE id=$1")
                    .bind(job.id)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO match_analysis_games(job_id,game_no,summary) VALUES($1,1,'{\"completed_games\":1,\"total_games\":1}')")
                    .bind(job.id).execute(&mut *tx).await.unwrap();
                // The connection misses the terminal notification. Its new LISTEN
                // must reconcile the committed result even without another event.
                sqlx::query("ALTER TABLE job_events DISABLE TRIGGER job_events_notify")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                let stopped: Vec<bool> = sqlx::query_scalar("SELECT pg_terminate_backend(pid,5000) FROM pg_stat_activity WHERE datname=current_database() AND application_name='neondojo-job-events'")
                    .fetch_all(&mut *tx).await.unwrap();
                assert_eq!(stopped, [true]);
            }
            sqlx::query("INSERT INTO job_events(job_id,event,data) VALUES($1,$2,jsonb_build_object('status',$3::text,'progress',$4::int))")
                .bind(job.id).bind(event).bind(status).bind(progress).execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
            expect_snapshot(&mut rx, match_id, job.id, status, progress).await;
        }
        reader.abort();
        facade.state().shutdown.cancel();
    }
}
