//! Committed Library analysis progress for visible matches and tracked bulk jobs.
use super::BackendFacade;
use crate::{databases::LibraryBulkJobStatus, domain_error::DomainError};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

#[derive(PartialEq, Eq)]
pub struct LibraryJobScope {
    pub rows: Vec<Uuid>,
    pub bulk: Vec<Uuid>,
    /// At least one visible match currently has queued or running analysis.
    pub pending: bool,
}

pub struct LibraryJobSnapshot {
    pub rows: Vec<(Uuid, Option<String>)>,
    pub missing_row: bool,
    pub bulk: Vec<LibraryBulkJobStatus>,
}

impl LibraryJobScope {
    async fn snapshot(
        &self,
        facade: &BackendFacade,
        changed_job: Option<Uuid>,
    ) -> Result<LibraryJobSnapshot, DomainError> {
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
        Ok(LibraryJobSnapshot {
            rows,
            missing_row,
            bulk,
        })
    }
}

impl BackendFacade {
    /// Follow the current Library page until its receiver closes or the task is cancelled.
    pub async fn follow_library_jobs(
        &self,
        scope: &LibraryJobScope,
        tx: mpsc::Sender<LibraryJobSnapshot>,
    ) {
        // Subscribe before reading so commits during a query are queued.
        let mut hints = self.state.subscribe_job_updates();
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
            let result = scope.snapshot(self, changed_job).await;
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
                _ = safety_read.tick(), if retry || scope.pending || !scope.bulk.is_empty() => Ok(None),
                _ = tx.closed() => return,
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
