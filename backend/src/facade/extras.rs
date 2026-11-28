//! Benchmarks, engine match duels, and direct path-based database backup/restore facade methods.

use super::BackendFacade;
use crate::{
    benchmarks::{
        self, BenchmarkHistory, BenchmarkReference, BenchmarkRequest, BenchmarkRun,
        BenchmarkStatus, PlayBenchmarkRequest,
    },
    database_backup::{self, BackupCheck, BackupTargetCounts},
    domain_error::DomainError,
    engine_matches::{self, CreateSeries, Series, SeriesDetails, SeriesPage},
    model::MatchSummary,
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

impl BackendFacade {
    pub fn benchmark_reference(&self) -> BenchmarkReference {
        benchmarks::reference()
    }

    pub async fn list_benchmarks(&self) -> Result<BenchmarkHistory, DomainError> {
        let _guard = self.db_access_guard()?;
        benchmarks::list_history(&self.state.pool).await
    }

    pub async fn start_benchmark(
        &self,
        request: BenchmarkRequest,
    ) -> Result<Vec<BenchmarkRun>, DomainError> {
        let _guard = self.db_access_guard()?;
        benchmarks::start_analysis_benchmarks(&self.state, request).await
    }

    pub async fn start_play_benchmark(
        &self,
        request: Option<PlayBenchmarkRequest>,
    ) -> Result<BenchmarkRun, DomainError> {
        let _guard = self.db_access_guard()?;
        benchmarks::start_play_benchmark(&self.state, request).await
    }

    pub async fn cancel_benchmark(&self, id: Uuid) -> Result<BenchmarkRun, DomainError> {
        let _guard = self.db_access_guard()?;
        benchmarks::cancel_benchmark(&self.state, id).await
    }

    pub async fn delete_benchmark(&self, id: Uuid) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        benchmarks::remove_benchmark(&self.state, id).await
    }

    pub async fn benchmark_status(&self) -> Result<BenchmarkStatus, DomainError> {
        let _guard = self.db_access_guard()?;
        benchmarks::read_status(&self.state.pool).await
    }

    pub async fn list_engine_matches(
        &self,
        before: Option<Uuid>,
    ) -> Result<SeriesPage, DomainError> {
        let _guard = self.db_access_guard()?;
        engine_matches::list_series(&self.state.pool, before).await
    }

    pub async fn get_engine_match(
        &self,
        id: Uuid,
        offset: i32,
    ) -> Result<SeriesDetails, DomainError> {
        let _guard = self.db_access_guard()?;
        engine_matches::series_details(&self.state.pool, id, offset).await
    }

    pub async fn start_engine_match(&self, request: CreateSeries) -> Result<Series, DomainError> {
        let _guard = self.db_access_guard()?;
        engine_matches::create_series(&self.state, request).await
    }

    pub async fn cancel_engine_match(&self, id: Uuid) -> Result<Series, DomainError> {
        let _guard = self.db_access_guard()?;
        engine_matches::cancel_series(&self.state, id).await
    }

    pub async fn resume_engine_match(&self, id: Uuid) -> Result<Series, DomainError> {
        let _guard = self.db_access_guard()?;
        engine_matches::resume_series(&self.state, id).await
    }

    pub async fn delete_engine_match(&self, id: Uuid) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        engine_matches::delete_series(&self.state, id).await
    }

    pub async fn engine_match_sgf(&self, id: Uuid, index: i32) -> Result<String, DomainError> {
        let _guard = self.db_access_guard()?;
        engine_matches::series_sgf(&self.state.pool, id, index).await
    }

    pub async fn engine_match_library_entry(
        &self,
        id: Uuid,
        index: i32,
    ) -> Result<MatchSummary, DomainError> {
        let _guard = self.db_access_guard()?;
        engine_matches::series_library_match(&self.state.pool, id, index).await
    }

    pub async fn backup_target_counts(&self) -> Result<BackupTargetCounts, DomainError> {
        let _guard = self.db_access_guard()?;
        database_backup::read_target_counts(&self.state.pool).await
    }

    /// Exports a full portable NeonDojo database backup directly to a local file
    /// path (or directory) without streaming through HTTP.
    pub async fn export_backup_to_path(&self, destination: &Path) -> Result<PathBuf, DomainError> {
        database_backup::export_to_path(&self.state, destination).await
    }

    /// Inspects a local `.neondojo-backup` archive directly from disk using
    /// `state.backup.transfer` concurrency protection.
    pub async fn check_backup_path(&self, path: &Path) -> Result<BackupCheck, DomainError> {
        database_backup::check_path(&self.state, path).await
    }

    /// Restores a local `.neondojo-backup` archive directly from disk, taking
    /// the exclusive restore admission and request-drain write lock.
    pub async fn restore_backup_path(&self, path: &Path, replace: bool) -> Result<(), DomainError> {
        database_backup::restore_path(&self.state, path, replace).await
    }
}
