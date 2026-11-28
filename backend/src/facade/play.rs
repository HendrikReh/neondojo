//! Live bot-play session lifecycle, actions, mode/feedback, review, SGF export,
//! and legal checker-path facade methods.

use super::BackendFacade;
use crate::{
    domain_error::DomainError,
    play::{
        self, CheckerTurn, CreatePlaySession, FeedbackOperation, PlayAction, PlayActionRequest,
        PlayFeedbackRequest, PlayHistorySnapshots, PlayModeRequest, PlayReviewRequest, PlaySession,
    },
};
use uuid::Uuid;

impl BackendFacade {
    /// Synchronous, database-free legal checker turn paths in absolute coordinates.
    pub fn checker_turns(
        &self,
        board: &[i32; 26],
        player: i32,
        dice: [i32; 2],
    ) -> Result<Vec<CheckerTurn>, DomainError> {
        play::compute_checker_turns(board, player, dice)
    }

    pub async fn list_play_sessions(&self) -> Result<Vec<PlaySession>, DomainError> {
        let _guard = self.db_access_guard()?;
        play::list_sessions_service(&self.state).await
    }

    pub async fn get_play_session(&self, id: Uuid) -> Result<PlaySession, DomainError> {
        let _guard = self.db_access_guard()?;
        play::get_session_service(&self.state, id).await
    }

    pub async fn create_play_session(
        &self,
        request: CreatePlaySession,
    ) -> Result<PlaySession, DomainError> {
        let _guard = self.db_access_guard()?;
        play::create_session_service(&self.state, request).await
    }

    pub async fn resume_play_session(
        &self,
        id: Uuid,
        version: i64,
    ) -> Result<PlaySession, DomainError> {
        let _guard = self.db_access_guard()?;
        play::act_in_session_service(
            &self.state,
            id,
            PlayActionRequest {
                decision_id: None,
                version,
                action: PlayAction::Resume,
                move_index: None,
                resign_level: None,
            },
        )
        .await
    }

    pub async fn pause_play_session(
        &self,
        id: Uuid,
        version: i64,
    ) -> Result<PlaySession, DomainError> {
        let _guard = self.db_access_guard()?;
        play::act_in_session_service(
            &self.state,
            id,
            PlayActionRequest {
                decision_id: None,
                version,
                action: PlayAction::Pause,
                move_index: None,
                resign_level: None,
            },
        )
        .await
    }

    pub async fn act_in_play_session(
        &self,
        id: Uuid,
        request: PlayActionRequest,
    ) -> Result<PlaySession, DomainError> {
        let _guard = self.db_access_guard()?;
        play::act_in_session_service(&self.state, id, request).await
    }

    pub async fn change_play_mode(
        &self,
        id: Uuid,
        request: PlayModeRequest,
    ) -> Result<PlaySession, DomainError> {
        let _guard = self.db_access_guard()?;
        play::change_mode_service(&self.state, id, request).await
    }

    pub async fn update_play_feedback(
        &self,
        id: Uuid,
        request: PlayFeedbackRequest,
    ) -> Result<PlaySession, DomainError> {
        let _guard = self.db_access_guard()?;
        play::update_feedback_service(&self.state, id, request).await
    }

    pub async fn play_hint(
        &self,
        id: Uuid,
        version: i64,
        decision_id: Option<Uuid>,
    ) -> Result<PlaySession, DomainError> {
        let _guard = self.db_access_guard()?;
        play::update_feedback_service(
            &self.state,
            id,
            PlayFeedbackRequest {
                version,
                operation: FeedbackOperation::Hint,
                decision_id,
            },
        )
        .await
    }

    pub async fn play_history(
        &self,
        id: Uuid,
        version: i64,
    ) -> Result<PlayHistorySnapshots, DomainError> {
        let _guard = self.db_access_guard()?;
        play::history_snapshots_service(&self.state, id, version).await
    }

    pub async fn review_play_session(
        &self,
        id: Uuid,
        request: PlayReviewRequest,
    ) -> Result<PlaySession, DomainError> {
        let _guard = self.db_access_guard()?;
        play::review_session_service(&self.state, id, request).await
    }

    pub async fn play_session_sgf(&self, id: Uuid) -> Result<String, DomainError> {
        let _guard = self.db_access_guard()?;
        play::export_session_sgf_service(&self.state, id).await
    }

    pub async fn delete_play_session(&self, id: Uuid) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        play::delete_session_service(&self.state, id).await
    }
}
