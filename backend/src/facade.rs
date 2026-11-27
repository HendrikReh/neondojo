//! Typed in-process backend facade for native desktop clients (GPUI).
//!
//! Every database-backed method acquires the same shared restore lock as the
//! HTTP `database_access` middleware (`state.backup.requests.try_read_owned()`)
//! so database restoration drains in-process desktop requests identically.

use crate::{
    api::{self, AppState, Health, JobStatusBatch, VersionInfo},
    domain_error::DomainError,
    model::{Job, JobEvent},
};
use futures_util::Stream;
use std::sync::Arc;
use tokio::sync::OwnedRwLockReadGuard;
use uuid::Uuid;

#[derive(Clone)]
pub struct BackendFacade {
    state: AppState,
}

impl BackendFacade {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Acquires a non-blocking shared read guard against active database
    /// restores. Mirrors Axum's `database_access` middleware.
    pub fn db_access_guard(&self) -> Result<OwnedRwLockReadGuard<()>, DomainError> {
        Arc::clone(&self.state.backup.requests)
            .try_read_owned()
            .map_err(|_| DomainError::restore_in_progress())
    }

    pub fn version(&self) -> VersionInfo {
        api::version_info()
    }

    pub async fn health(&self) -> Result<Health, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(api::check_health(&self.state).await.1)
    }

    pub async fn get_job(&self, id: Uuid) -> Result<Job, DomainError> {
        let _guard = self.db_access_guard()?;
        api::get_job_by_id(&self.state, id).await
    }

    pub async fn cancel_job(&self, id: Uuid) -> Result<Job, DomainError> {
        let _guard = self.db_access_guard()?;
        api::cancel_job_by_id(&self.state, id).await
    }

    pub async fn job_status_batch(&self, ids: Vec<Uuid>) -> Result<JobStatusBatch, DomainError> {
        let _guard = self.db_access_guard()?;
        api::get_job_status_batch(&self.state, ids).await
    }

    pub async fn watch_job_events(
        &self,
        id: Uuid,
        after: i64,
    ) -> Result<impl Stream<Item = JobEvent> + Send + 'static + use<>, DomainError> {
        let _guard = self.db_access_guard()?;
        api::watch_job_events_checked(&self.state, id, after).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain_error::ErrorKind, engine::Engine};
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

    fn lazy_facade() -> BackendFacade {
        let pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        BackendFacade::new(AppState::new(pool, Engine::new("missing-gnubg".into())))
    }

    #[tokio::test]
    async fn version_returns_package_release() {
        let facade = lazy_facade();
        let info = facade.version();
        assert_eq!(info.release, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.commit, info.version);
    }

    #[tokio::test]
    async fn db_access_guard_rejects_calls_while_restore_holds_write_lock() {
        let facade = lazy_facade();
        assert!(facade.db_access_guard().is_ok());

        let restore_lock = facade.state().backup.requests.write().await;
        let err = facade.db_access_guard().unwrap_err();
        assert_eq!(err.kind, ErrorKind::ServiceUnavailable);
        assert_eq!(err.body.error, "Database restoration is in progress");

        drop(restore_lock);
        assert!(facade.db_access_guard().is_ok());
    }
}
