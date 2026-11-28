//! Settings, personal configuration, named profiles, engine checks, and opening book facade methods.

use super::BackendFacade;
use crate::{
    configuration::{
        self, ConfigurationAvailability, ConfigurationProfile, ConfigurationState,
        InitializeConfiguration, RestoreConfiguration, UpdateConfiguration,
    },
    domain_error::DomainError,
    opening_book::{self, BookConfigWrite, BookSettings, BookStatus, xg_book::XgBookStatus},
    profiles::{
        self, ActivateConfigurationProfile, ConfigurationValidation, CreateConfigurationProfile,
        DeleteConfigurationProfile, RenameConfigurationProfile, SaveConfigurationProfile,
        ValidateConfigurationProfile,
    },
    settings::{self, AppSettings, EngineCheckResponse, EngineInfo, OpenSageInstallation},
};
use uuid::Uuid;

impl BackendFacade {
    pub async fn get_configuration(&self) -> Result<ConfigurationState, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(configuration::store::read(&self.state.pool).await?)
    }

    pub async fn save_configuration(
        &self,
        input: UpdateConfiguration,
    ) -> Result<ConfigurationState, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(configuration::store::update(&self.state, input).await?)
    }

    pub async fn initialize_configuration(
        &self,
        input: InitializeConfiguration,
    ) -> Result<ConfigurationState, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(configuration::store::initialize(&self.state, input).await?)
    }

    pub async fn restore_configuration(
        &self,
        input: RestoreConfiguration,
    ) -> Result<ConfigurationState, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(configuration::store::restore(&self.state, input).await?)
    }

    pub async fn configuration_availability(
        &self,
    ) -> Result<ConfigurationAvailability, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(configuration::availability(&self.state).await?)
    }

    pub async fn get_settings(&self) -> Result<AppSettings, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(settings::load(&self.state).await?)
    }

    pub async fn save_settings(&self, settings: AppSettings) -> Result<AppSettings, DomainError> {
        let _guard = self.db_access_guard()?;
        settings::save_settings(&self.state, settings).await
    }

    pub async fn list_engines(&self) -> Result<Vec<EngineInfo>, DomainError> {
        let _guard = self.db_access_guard()?;
        settings::list_engines(&self.state).await
    }

    pub async fn check_gnubg(
        &self,
        path: String,
        threads: Option<i32>,
    ) -> Result<EngineCheckResponse, DomainError> {
        settings::check_gnubg_installation(path, threads).await
    }

    pub async fn get_open_sage(&self) -> Result<OpenSageInstallation, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(settings::open_sage::load(&self.state.pool).await?)
    }

    pub async fn verify_open_sage(
        &self,
        version: i64,
        path: String,
    ) -> Result<OpenSageInstallation, DomainError> {
        let _guard = self.db_access_guard()?;
        settings::open_sage::verify_installation(&self.state, version, path).await
    }

    pub async fn list_profiles(&self) -> Result<Vec<ConfigurationProfile>, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(profiles::list(&self.state).await?)
    }

    pub async fn get_profile(&self, id: Uuid) -> Result<ConfigurationProfile, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(profiles::get_by_id(&self.state, id).await?)
    }

    pub async fn create_profile(
        &self,
        input: CreateConfigurationProfile,
    ) -> Result<ConfigurationState, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(profiles::create(&self.state, input).await?)
    }

    pub async fn update_profile(
        &self,
        id: Uuid,
        input: RenameConfigurationProfile,
    ) -> Result<ConfigurationState, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(profiles::rename(&self.state, id, input).await?)
    }

    pub async fn save_profile_snapshot(
        &self,
        id: Uuid,
        input: SaveConfigurationProfile,
    ) -> Result<ConfigurationState, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(profiles::save_snapshot(&self.state, id, input).await?)
    }

    pub async fn delete_profile(
        &self,
        id: Uuid,
        input: DeleteConfigurationProfile,
    ) -> Result<ConfigurationState, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(profiles::delete(&self.state, id, input).await?)
    }

    pub async fn validate_profile(
        &self,
        id: Uuid,
        input: ValidateConfigurationProfile,
    ) -> Result<ConfigurationValidation, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(profiles::validate(&self.state, id, input).await?)
    }

    pub async fn activate_profile(
        &self,
        id: Uuid,
        input: ActivateConfigurationProfile,
    ) -> Result<ConfigurationState, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(profiles::activate(&self.state, id, input).await?)
    }

    pub async fn get_opening_book(&self) -> Result<BookSettings, DomainError> {
        let _guard = self.db_access_guard()?;
        opening_book::read_settings(&self.state).await
    }

    pub async fn save_opening_book(
        &self,
        config: BookConfigWrite,
    ) -> Result<BookSettings, DomainError> {
        let _guard = self.db_access_guard()?;
        opening_book::write_settings(&self.state, config).await
    }

    pub async fn check_opening_book(&self, path: String) -> Result<BookStatus, DomainError> {
        opening_book::check_book_path(path).await
    }

    pub async fn install_opening_book(&self, path: String) -> Result<BookStatus, DomainError> {
        let _guard = self.db_access_guard()?;
        opening_book::install_book_path(&self.state, path).await
    }

    pub async fn check_xg_opening_book(&self, path: String) -> Result<XgBookStatus, DomainError> {
        opening_book::check_xg_book_path(&self.state, path).await
    }

    pub async fn install_xg_opening_book(&self, path: String) -> Result<XgBookStatus, DomainError> {
        let _guard = self.db_access_guard()?;
        opening_book::install_xg_book_path(&self.state, path).await
    }
}
