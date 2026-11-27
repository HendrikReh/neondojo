//! Player directory and virtual player profile facade methods.

use super::BackendFacade;
use crate::{
    domain_error::DomainError,
    player_directory::{
        self, MyPlayerProfile, PlayerAliasWrite, PlayerDetails, PlayerMerge, PlayerNameWrite,
        PlayerPage, PlayerQuery, VirtualPlayerDetails, VirtualPlayerList,
    },
};
use uuid::Uuid;

impl BackendFacade {
    pub async fn list_players(&self, query: PlayerQuery) -> Result<PlayerPage, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::list_players_service(&self.state.pool, query).await
    }

    pub async fn get_player_summary(&self, id: Uuid) -> Result<PlayerDetails, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::get_player_details_service(&self.state.pool, id).await
    }

    pub async fn update_player(
        &self,
        id: Uuid,
        input: PlayerNameWrite,
    ) -> Result<PlayerDetails, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::update_player_service(&self.state.pool, id, input).await
    }

    pub async fn merge_players(
        &self,
        id: Uuid,
        input: PlayerMerge,
    ) -> Result<PlayerDetails, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::merge_players_service(&self.state.pool, id, input).await
    }

    pub async fn add_player_alias(
        &self,
        id: Uuid,
        input: PlayerAliasWrite,
    ) -> Result<PlayerDetails, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::add_player_alias_service(&self.state.pool, id, input).await
    }

    pub async fn list_virtual_profiles(&self) -> Result<VirtualPlayerList, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::list_virtual_service(&self.state.pool).await
    }

    pub async fn get_virtual_profile(&self, id: Uuid) -> Result<VirtualPlayerDetails, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::get_virtual_service(&self.state.pool, id).await
    }

    pub async fn create_virtual_profile(
        &self,
        name: String,
    ) -> Result<VirtualPlayerDetails, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::create_virtual_service(&self.state.pool, &name).await
    }

    pub async fn update_virtual_profile(
        &self,
        id: Uuid,
        name: String,
        version: i64,
    ) -> Result<VirtualPlayerDetails, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::rename_virtual_service(&self.state.pool, id, &name, version).await
    }

    pub async fn delete_virtual_profile(&self, id: Uuid, version: i64) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::delete_virtual_service(&self.state.pool, id, version).await
    }

    pub async fn add_virtual_member(
        &self,
        id: Uuid,
        profile_id: Uuid,
        version: i64,
    ) -> Result<VirtualPlayerDetails, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::add_virtual_member_service(&self.state.pool, id, profile_id, version)
            .await
    }

    pub async fn remove_virtual_member(
        &self,
        id: Uuid,
        profile_id: Uuid,
        version: i64,
    ) -> Result<VirtualPlayerDetails, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::remove_virtual_member_service(&self.state.pool, id, profile_id, version)
            .await
    }

    pub async fn get_my_player_profile(&self) -> Result<MyPlayerProfile, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::get_my_profile_service(&self.state.pool).await
    }

    pub async fn set_my_player_profile(
        &self,
        id: Option<Uuid>,
    ) -> Result<MyPlayerProfile, DomainError> {
        let _guard = self.db_access_guard()?;
        player_directory::set_my_profile_service(&self.state.pool, id).await
    }
}
