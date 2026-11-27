//! Match library, databases, direct path import/export, and duplicate finder facade methods.

use super::BackendFacade;
use crate::{
    api,
    databases::{
        self, ActionResponse, Database, DatabaseMatch, DatabaseWrite, ExportFormat, ExportRequest,
        LibraryAction, LibraryPage, LibraryQuery, MetadataWrite,
    },
    domain_error::DomainError,
    duplicate_matches::{self, DuplicateMatchPage, DuplicateMatchQuery},
    model::{ImportResponse, MatchSummary},
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

impl BackendFacade {
    pub async fn list_databases(&self) -> Result<Vec<Database>, DomainError> {
        let _guard = self.db_access_guard()?;
        databases::list_databases_service(&self.state.pool).await
    }

    pub async fn create_database(&self, input: DatabaseWrite) -> Result<Database, DomainError> {
        let _guard = self.db_access_guard()?;
        databases::create_database_service(&self.state.pool, input).await
    }

    pub async fn rename_database(
        &self,
        id: Uuid,
        input: DatabaseWrite,
    ) -> Result<Database, DomainError> {
        let _guard = self.db_access_guard()?;
        databases::update_database_service(&self.state.pool, id, input).await
    }

    pub async fn delete_database(
        &self,
        id: Uuid,
        version: i64,
        delete_matches: bool,
    ) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        databases::delete_database_service(&self.state.pool, id, version, delete_matches).await
    }

    pub async fn empty_database(&self, id: Uuid, version: i64) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        databases::clear_database_service(&self.state.pool, id, version).await
    }

    pub async fn search_matches(&self, query: LibraryQuery) -> Result<LibraryPage, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(databases::browse(&self.state.pool, query).await?)
    }

    pub async fn get_match(&self, id: Uuid) -> Result<MatchSummary, DomainError> {
        let _guard = self.db_access_guard()?;
        api::get_match_by_id(&self.state, id).await
    }

    pub async fn get_match_metadata(&self, id: Uuid) -> Result<DatabaseMatch, DomainError> {
        let _guard = self.db_access_guard()?;
        databases::get_metadata_service(&self.state.pool, id).await
    }

    pub async fn update_match_metadata(
        &self,
        id: Uuid,
        input: MetadataWrite,
    ) -> Result<DatabaseMatch, DomainError> {
        let _guard = self.db_access_guard()?;
        databases::update_metadata_service(&self.state, id, input).await
    }

    pub async fn apply_library_action(
        &self,
        action: LibraryAction,
    ) -> Result<ActionResponse, DomainError> {
        let _guard = self.db_access_guard()?;
        databases::apply_actions_service(&self.state, action).await
    }

    pub async fn delete_matches(
        &self,
        match_ids: Vec<Uuid>,
        database_id: Option<Uuid>,
    ) -> Result<ActionResponse, DomainError> {
        self.apply_library_action(LibraryAction {
            action: "delete".into(),
            match_ids,
            database_id,
            target_database_id: None,
            add_tags: Vec::new(),
            remove_tags: Vec::new(),
        })
        .await
    }

    pub async fn move_matches(
        &self,
        match_ids: Vec<Uuid>,
        database_id: Option<Uuid>,
        target_database_id: Uuid,
    ) -> Result<ActionResponse, DomainError> {
        self.apply_library_action(LibraryAction {
            action: "move".into(),
            match_ids,
            database_id,
            target_database_id: Some(target_database_id),
            add_tags: Vec::new(),
            remove_tags: Vec::new(),
        })
        .await
    }

    pub async fn copy_matches(
        &self,
        match_ids: Vec<Uuid>,
        database_id: Option<Uuid>,
        target_database_id: Uuid,
    ) -> Result<ActionResponse, DomainError> {
        self.apply_library_action(LibraryAction {
            action: "copy".into(),
            match_ids,
            database_id,
            target_database_id: Some(target_database_id),
            add_tags: Vec::new(),
            remove_tags: Vec::new(),
        })
        .await
    }

    pub async fn find_duplicates(
        &self,
        query: DuplicateMatchQuery,
    ) -> Result<DuplicateMatchPage, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(duplicate_matches::find(&self.state.pool, query).await?)
    }

    /// Imports a local match file directly from disk, bypassing HTTP multipart
    /// framing and the 16 MiB Axum body limit.
    pub async fn import_match_path(
        &self,
        path: &Path,
        database_id: Option<Uuid>,
    ) -> Result<ImportResponse, DomainError> {
        let _guard = self.db_access_guard()?;
        api::import_match_from_path(&self.state, path, database_id).await
    }

    /// Exports one match (original/SGF/XG) or multiple matches (ZIP) directly
    /// to `destination` (either a target file path or an existing directory).
    pub async fn export_matches_to_path(
        &self,
        match_ids: Vec<Uuid>,
        format: ExportFormat,
        destination: &Path,
    ) -> Result<PathBuf, DomainError> {
        let _guard = self.db_access_guard()?;
        databases::export_to_path_service(
            &self.state.pool,
            ExportRequest { match_ids, format },
            destination,
        )
        .await
    }
}
