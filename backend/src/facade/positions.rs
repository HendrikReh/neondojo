//! Saved positions browser, bookmarks, saved filters, decision search,
//! deck additions, and similar-positions practice facade methods.

use super::BackendFacade;
use crate::{
    domain_error::DomainError,
    learning::{self, AddDecisions, AddDecisionsResult, AddPositions, AddPositionsResult},
    position_bookmarks::{self, PositionBookmark},
    position_browser::{
        self, CreatePositionFilter, PositionAnalysisReadiness, PositionBrowserPage,
        PositionFilterDefinition, SavedPositionFilter, UpdatePositionFilter,
    },
    recurring_mistakes::{self, DecisionSearchPage, DecisionSearchQuery},
    similar_positions::{
        self, CheckerPracticeAnswer, CheckerPracticeMove, CheckerPracticeScore,
        SaveSimilarPracticeSummary, SimilarPositions, SimilarPracticeHistory,
        SimilarPracticeSummary, SimilarQuery,
    },
};
use uuid::Uuid;

impl BackendFacade {
    // ── Saved Positions & Bookmarks ──────────────────────────────────────────

    pub async fn search_saved_positions(
        &self,
        filters: PositionFilterDefinition,
        after: Option<String>,
        page: Option<i64>,
        limit: Option<i64>,
    ) -> Result<PositionBrowserPage, DomainError> {
        let _guard = self.db_access_guard()?;
        position_browser::search_saved_positions(&self.state, filters, after, page, limit).await
    }

    pub async fn position_analysis_readiness(
        &self,
        position_ids: Vec<Uuid>,
    ) -> Result<PositionAnalysisReadiness, DomainError> {
        let _guard = self.db_access_guard()?;
        position_browser::check_analysis_readiness(&self.state, AddPositions { position_ids }).await
    }

    pub async fn get_position_bookmark(
        &self,
        position_id: Uuid,
    ) -> Result<PositionBookmark, DomainError> {
        let _guard = self.db_access_guard()?;
        position_bookmarks::read_bookmark(&self.state, position_id).await
    }

    pub async fn bookmark_position(
        &self,
        position_id: Uuid,
    ) -> Result<PositionBookmark, DomainError> {
        let _guard = self.db_access_guard()?;
        position_bookmarks::create_bookmark(&self.state, position_id).await
    }

    pub async fn unbookmark_position(&self, position_id: Uuid) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        position_bookmarks::delete_bookmark(&self.state, position_id).await
    }

    pub async fn batch_remove_bookmarks(
        &self,
        position_ids: Vec<Uuid>,
    ) -> Result<u64, DomainError> {
        let _guard = self.db_access_guard()?;
        position_bookmarks::batch_delete_bookmarks(&self.state, position_ids).await
    }

    // ── Saved Position Filters / Searches ────────────────────────────────────

    pub async fn list_saved_searches(&self) -> Result<Vec<SavedPositionFilter>, DomainError> {
        let _guard = self.db_access_guard()?;
        position_browser::list_saved_filters(&self.state).await
    }

    pub async fn save_position_search(
        &self,
        input: CreatePositionFilter,
    ) -> Result<SavedPositionFilter, DomainError> {
        let _guard = self.db_access_guard()?;
        position_browser::create_saved_filter(&self.state, input).await
    }

    pub async fn update_saved_search(
        &self,
        id: Uuid,
        input: UpdatePositionFilter,
    ) -> Result<SavedPositionFilter, DomainError> {
        let _guard = self.db_access_guard()?;
        position_browser::update_saved_filter(&self.state, id, input).await
    }

    pub async fn delete_saved_search(
        &self,
        id: Uuid,
        expected_version: i64,
    ) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        position_browser::delete_saved_filter(&self.state, id, expected_version).await
    }

    // ── Decisions Search & Deck Additions ────────────────────────────────────

    pub async fn add_positions_to_deck(
        &self,
        deck_id: Uuid,
        position_ids: Vec<Uuid>,
    ) -> Result<AddPositionsResult, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::add_positions_to_deck_service(&self.state, deck_id, AddPositions { position_ids })
            .await
    }

    pub async fn search_decisions(
        &self,
        query: DecisionSearchQuery,
    ) -> Result<DecisionSearchPage, DomainError> {
        let _guard = self.db_access_guard()?;
        recurring_mistakes::search_decisions_service(&self.state, query).await
    }

    pub async fn add_decisions_to_deck(
        &self,
        deck_id: Uuid,
        decisions: Vec<recurring_mistakes::MistakeReference>,
    ) -> Result<AddDecisionsResult, DomainError> {
        let _guard = self.db_access_guard()?;
        learning::add_decisions_to_deck_service(&self.state, deck_id, AddDecisions { decisions })
            .await
    }

    // ── Similar Positions & Practice ─────────────────────────────────────────

    pub async fn find_similar_positions(
        &self,
        position_id: Uuid,
        query: SimilarQuery,
    ) -> Result<SimilarPositions, DomainError> {
        let _guard = self.db_access_guard()?;
        similar_positions::find_similar(&self.state, position_id, query).await
    }

    pub async fn reveal_similar_practice_answer(
        &self,
        position_id: Uuid,
    ) -> Result<CheckerPracticeAnswer, DomainError> {
        let _guard = self.db_access_guard()?;
        similar_positions::reveal_practice_answer(&self.state, position_id).await
    }

    pub async fn evaluate_similar_practice(
        &self,
        position_id: Uuid,
        input: CheckerPracticeMove,
    ) -> Result<CheckerPracticeScore, DomainError> {
        let _guard = self.db_access_guard()?;
        similar_positions::score_practice_move(&self.state, position_id, input).await
    }

    pub async fn save_similar_practice_summary(
        &self,
        summary: SaveSimilarPracticeSummary,
    ) -> Result<SimilarPracticeSummary, DomainError> {
        let _guard = self.db_access_guard()?;
        similar_positions::save_similar_practice_summary_service(&self.state, summary).await
    }

    pub async fn list_similar_practice_history(
        &self,
        before: Option<Uuid>,
    ) -> Result<SimilarPracticeHistory, DomainError> {
        let _guard = self.db_access_guard()?;
        similar_positions::list_similar_practice_history_service(&self.state, before).await
    }
}
