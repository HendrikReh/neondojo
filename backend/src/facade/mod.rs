//! Typed in-process backend facade for native desktop clients (GPUI).
//!
//! Every database-backed method (except database backup export/check/restore,
//! which coordinate through `state.backup.transfer` and exclusive restore
//! locks) acquires the same shared restore lock as the HTTP `database_access`
//! middleware (`state.backup.requests.try_read_owned()`) so database
//! restoration drains in-process desktop requests identically.

mod analysis;
mod extras;
mod library;
mod play;
mod players;
mod positions;
mod settings;
mod study;

use crate::{
    api::{self, AppState, Health, JobStatusBatch, VersionInfo},
    domain_error::DomainError,
    model::{Job, JobEvent},
};
use futures_util::Stream;
use std::sync::Arc;
use tokio::sync::{OwnedRwLockReadGuard, broadcast};
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

    pub fn subscribe_benchmark_updates(&self) -> broadcast::Receiver<Uuid> {
        self.state.subscribe_benchmark_updates()
    }

    pub fn subscribe_engine_match_updates(&self) -> broadcast::Receiver<Uuid> {
        self.state.subscribe_engine_match_updates()
    }

    pub fn subscribe_assessment_updates(&self) -> broadcast::Receiver<Uuid> {
        self.state.subscribe_assessment_updates()
    }

    pub fn subscribe_anki_sync_updates(&self) -> broadcast::Receiver<Option<Uuid>> {
        self.state.subscribe_anki_sync_updates()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        analysis_catalog::AnalysisCatalogQuery, annotations::AnnotationKind,
        databases::LibraryQuery, domain_error::ErrorKind, engine::Engine, learning::CardQuery,
        player_directory::PlayerQuery, position_browser::PositionFilterDefinition,
        recurring_mistakes::MistakesQuery, similar_positions::SimilarQuery, study::ReportQuery,
    };
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use std::path::Path;

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

        // Every DB-backed domain facade method must fail fast with 503 during restore.
        let id = Uuid::new_v4();
        for result in [
            facade.get_configuration().await.err(),
            facade.get_settings().await.err(),
            facade.list_profiles().await.err(),
            facade.get_opening_book().await.err(),
            facade.list_databases().await.err(),
            facade.search_matches(LibraryQuery::default()).await.err(),
            facade.list_players(PlayerQuery::default()).await.err(),
            facade.list_virtual_profiles().await.err(),
            facade.list_benchmarks().await.err(),
            facade.benchmark_status().await.err(),
            facade.list_engine_matches(None).await.err(),
            facade.backup_target_counts().await.err(),
            // Analysis facade
            facade.match_games(id).await.err(),
            facade
                .list_analysis_catalog(id, AnalysisCatalogQuery::default())
                .await
                .err(),
            facade
                .get_annotations(AnnotationKind::Match, id)
                .await
                .err(),
            // Play facade
            facade.list_play_sessions().await.err(),
            facade.get_play_session(id).await.err(),
            // Positions facade
            facade
                .search_saved_positions(PositionFilterDefinition::default(), None, None, None)
                .await
                .err(),
            facade.list_saved_searches().await.err(),
            facade
                .find_similar_positions(id, SimilarQuery::default())
                .await
                .err(),
            // Study / Learning / Anki facade
            facade.study_overview().await.err(),
            facade
                .list_study_reports(ReportQuery::default())
                .await
                .err(),
            facade
                .list_recurring_mistakes(MistakesQuery::default())
                .await
                .err(),
            facade.list_decks().await.err(),
            facade.search_cards(None, CardQuery::default()).await.err(),
            facade.anki_sync_overview().await.err(),
            facade.get_anki_card_face_html(id, "front").await.err(),
            facade.get_anki_media_blob("pkg", "img.png").await.err(),
        ] {
            let err = result.expect("call must be rejected during restore");
            assert_eq!(err.kind, ErrorKind::ServiceUnavailable);
            assert_eq!(err.body.error, "Database restoration is in progress");
        }

        drop(restore_lock);
        assert!(facade.db_access_guard().is_ok());
    }

    #[tokio::test]
    async fn push_broadcast_channels_deliver_updates() {
        let facade = lazy_facade();
        let mut bench_rx = facade.subscribe_benchmark_updates();
        let mut duel_rx = facade.subscribe_engine_match_updates();
        let mut assess_rx = facade.subscribe_assessment_updates();
        let mut anki_rx = facade.subscribe_anki_sync_updates();

        let id = Uuid::new_v4();
        facade.state().notify_benchmark_update(id);
        facade.state().notify_engine_match_update(id);
        facade.state().notify_assessment_update(id);
        facade.state().notify_anki_sync_update(Some(id));
        facade.state().notify_anki_sync_update(None);

        assert_eq!(bench_rx.recv().await.unwrap(), id);
        assert_eq!(duel_rx.recv().await.unwrap(), id);
        assert_eq!(assess_rx.recv().await.unwrap(), id);
        assert_eq!(anki_rx.recv().await.unwrap(), Some(id));
        assert_eq!(anki_rx.recv().await.unwrap(), None);
    }

    #[tokio::test]
    async fn path_based_import_and_backup_validate_inputs_without_http() {
        let facade = lazy_facade();
        let missing = Path::new("/nonexistent/neondojo-missing-file.sgf");
        assert_eq!(
            facade
                .import_match_path(missing, None)
                .await
                .unwrap_err()
                .kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            facade.check_backup_path(missing).await.unwrap_err().kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            facade
                .restore_backup_path(missing, false)
                .await
                .unwrap_err()
                .kind,
            ErrorKind::UnprocessableEntity
        );
        assert_eq!(
            facade
                .import_deck_apkg_from_path(Uuid::new_v4(), missing)
                .await
                .unwrap_err()
                .kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            facade
                .import_anki_package_from_path(missing)
                .await
                .unwrap_err()
                .kind,
            ErrorKind::NotFound
        );
        assert_eq!(
            facade
                .get_anki_card_face_html(Uuid::new_v4(), "sideways")
                .await
                .unwrap_err()
                .kind,
            ErrorKind::UnprocessableEntity
        );
    }

    #[tokio::test]
    async fn synchronous_domain_helpers_work_without_database() {
        let facade = lazy_facade();
        let board = crate::backgammon::initial_board();
        let plans = facade.classify_game_plans(&board).unwrap();
        assert_eq!(plans.white, crate::game_plan::GamePlan::Opening);
        assert_eq!(plans.black, crate::game_plan::GamePlan::Opening);

        let turns = facade.checker_turns(&board, 0, [3, 1]).unwrap();
        assert!(!turns.is_empty());

        let match_id = crate::backgammon::ids::match_id(7, [0, 0], 1, -1, false, false, 0, [0, 0]);
        let cube = facade.get_cube_context(&match_id, 0).unwrap();
        assert_eq!(cube.match_length, 7);
        assert_eq!(cube.cube_value, 1);
    }
}
