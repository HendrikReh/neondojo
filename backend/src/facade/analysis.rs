//! Analysis, match/game/position reads, review decisions, catalogs,
//! opening-book lookup, cube context/chances, game plans, explanations,
//! and annotations facade methods.

use super::BackendFacade;
use crate::{
    analysis_catalog::{
        self, AnalysisCatalogPage, AnalysisCatalogQuery, AnalysisSelection,
        AnalysisSelectionUpdate, SelectedPositionAnalysis,
    },
    analysis_statistics::{
        self, JobAnalysisStatistics,
        catalog::{CatalogStatisticsQuery, MatchAnalysisStatistics},
    },
    analysis_timing::{self, AnalysisRemainingTime},
    annotations::{
        self, Annotation, AnnotationKind, AnnotationPage, AnnotationUpdate, SearchQuery, TagQuery,
    },
    api::{
        self, JobRequest, MatchAnalysisEngine, MatchAnalysisQuery, MatchAnalysisRequest,
        MoveAnalysisQuery, PositionQuery,
    },
    benchmarks::estimate::{self, AnalysisTimeEstimate, EstimateQuery},
    cube_context::{self, CubeChances, CubeContext},
    domain_error::DomainError,
    game_plan::{self, GamePlans},
    imported_analysis::{self, ImportedAnalysisRecord, ImportedAnalysisSummary},
    model::{
        AnalysisComparisonRun, DiceStats, ImportWarning, Job, MatchGame, MoveAnalysisPage,
        Position, PositionPage,
    },
    move_assessments::{self, GameMoveAssessments, MoveAssessmentQuery},
    move_catalog::{self, GameDecisions, StoredMove},
    move_explanation::{self, MoveExplanation, MoveExplanationRequest},
    opening_book::{self, BookLookup},
    practice,
    review_decisions::{self, ReviewDecisionPage, ReviewQuery},
};
use uuid::Uuid;

impl BackendFacade {
    // ── Match, Game & Position Reads ─────────────────────────────────────────

    pub async fn match_games(&self, match_id: Uuid) -> Result<Vec<MatchGame>, DomainError> {
        let _guard = self.db_access_guard()?;
        api::get_match_games(&self.state, match_id).await
    }

    pub async fn match_import_warnings(
        &self,
        match_id: Uuid,
    ) -> Result<Vec<ImportWarning>, DomainError> {
        let _guard = self.db_access_guard()?;
        api::get_match_import_warnings(&self.state, match_id).await
    }

    pub async fn match_dice(
        &self,
        match_id: Uuid,
        game_no: Option<i32>,
    ) -> Result<DiceStats, DomainError> {
        let _guard = self.db_access_guard()?;
        api::get_match_dice(&self.state, match_id, game_no).await
    }

    /// Returns the stored SGF bytes for a match (same payload as `GET /api/matches/{id}/source`).
    pub async fn match_source_bytes(&self, match_id: Uuid) -> Result<Vec<u8>, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(api::get_match_source_sgf(&self.state, match_id)
            .await?
            .into_bytes())
    }

    pub async fn list_positions(&self, query: PositionQuery) -> Result<PositionPage, DomainError> {
        let _guard = self.db_access_guard()?;
        api::list_positions(&self.state, query).await
    }

    pub async fn get_position(&self, position_id: Uuid) -> Result<Position, DomainError> {
        let _guard = self.db_access_guard()?;
        api::get_position_by_id(&self.state, position_id).await
    }

    pub async fn list_position_jobs(
        &self,
        position_id: Uuid,
        engine_id: Option<MatchAnalysisEngine>,
    ) -> Result<Vec<Job>, DomainError> {
        let _guard = self.db_access_guard()?;
        analysis_catalog::list_position_jobs(&self.state, position_id, engine_id).await
    }

    pub async fn delete_position_job(
        &self,
        position_id: Uuid,
        job_id: Uuid,
    ) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        analysis_catalog::remove_position_job(&self.state, position_id, job_id).await
    }

    // ── Analysis Jobs, Catalog & Statistics ──────────────────────────────────

    pub async fn create_position_job(&self, request: JobRequest) -> Result<Job, DomainError> {
        let _guard = self.db_access_guard()?;
        api::submit_position_job(&self.state, request).await
    }

    pub async fn latest_match_analysis(
        &self,
        match_id: Uuid,
        query: MatchAnalysisQuery,
    ) -> Result<Option<Job>, DomainError> {
        let _guard = self.db_access_guard()?;
        api::get_latest_match_analysis(&self.state, match_id, query).await
    }

    pub async fn create_match_analysis(
        &self,
        match_id: Uuid,
        request: MatchAnalysisRequest,
    ) -> Result<Job, DomainError> {
        let _guard = self.db_access_guard()?;
        api::submit_match_analysis(&self.state, match_id, request).await
    }

    pub async fn list_match_analysis_jobs(
        &self,
        match_id: Uuid,
        game_no: Option<i32>,
    ) -> Result<Vec<AnalysisComparisonRun>, DomainError> {
        let _guard = self.db_access_guard()?;
        api::get_match_analysis_jobs(&self.state, match_id, game_no).await
    }

    pub async fn move_analyses(
        &self,
        job_id: Uuid,
        query: MoveAnalysisQuery,
    ) -> Result<MoveAnalysisPage, DomainError> {
        let _guard = self.db_access_guard()?;
        api::list_move_analyses(&self.state, job_id, query).await
    }

    pub async fn job_remaining_time(
        &self,
        job_id: Uuid,
    ) -> Result<AnalysisRemainingTime, DomainError> {
        let _guard = self.db_access_guard()?;
        analysis_timing::load_remaining_time(&self.state, job_id).await
    }

    pub async fn estimate_analysis_time(
        &self,
        match_id: Uuid,
        query: EstimateQuery,
    ) -> Result<AnalysisTimeEstimate, DomainError> {
        let _guard = self.db_access_guard()?;
        estimate::estimate_match_analysis(&self.state, match_id, query).await
    }

    pub async fn list_analysis_catalog(
        &self,
        match_id: Uuid,
        query: AnalysisCatalogQuery,
    ) -> Result<AnalysisCatalogPage, DomainError> {
        let _guard = self.db_access_guard()?;
        analysis_catalog::list_catalog(&self.state, match_id, query).await
    }

    pub async fn select_displayed_analysis(
        &self,
        match_id: Uuid,
        analysis_id: Option<String>,
    ) -> Result<AnalysisSelection, DomainError> {
        let _guard = self.db_access_guard()?;
        analysis_catalog::select_analysis(
            &self.state,
            match_id,
            AnalysisSelectionUpdate { analysis_id },
        )
        .await
    }

    pub async fn delete_analysis_entry(
        &self,
        match_id: Uuid,
        analysis_id: &str,
        source_revision: i64,
    ) -> Result<(), DomainError> {
        let _guard = self.db_access_guard()?;
        analysis_catalog::delete_analysis_entry(&self.state, match_id, analysis_id, source_revision)
            .await
    }

    pub async fn get_position_analysis_result(
        &self,
        position_id: Uuid,
        analysis_id: Option<&str>,
    ) -> Result<Option<SelectedPositionAnalysis>, DomainError> {
        let _guard = self.db_access_guard()?;
        analysis_catalog::load_position_result(&self.state, position_id, analysis_id).await
    }

    pub async fn get_match_statistics(
        &self,
        match_id: Uuid,
        query: CatalogStatisticsQuery,
    ) -> Result<MatchAnalysisStatistics, DomainError> {
        let _guard = self.db_access_guard()?;
        analysis_statistics::catalog::load_match_statistics(&self.state, match_id, query).await
    }

    pub async fn get_job_statistics(
        &self,
        job_id: Uuid,
    ) -> Result<JobAnalysisStatistics, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(analysis_statistics::load_statistics(&self.state, job_id).await?)
    }

    // ── Move Assessments, Move Catalog, Review & Imported Analysis ───────────

    pub async fn get_move_assessments(
        &self,
        match_id: Uuid,
        query: MoveAssessmentQuery,
    ) -> Result<GameMoveAssessments, DomainError> {
        let _guard = self.db_access_guard()?;
        move_assessments::load_move_assessments(&self.state, match_id, query).await
    }

    pub async fn get_move_catalog(
        &self,
        match_id: Uuid,
        game_no: i32,
    ) -> Result<GameDecisions, DomainError> {
        let _guard = self.db_access_guard()?;
        move_catalog::load_game_decisions(&self.state, match_id, game_no).await
    }

    pub async fn get_move(&self, move_id: Uuid) -> Result<StoredMove, DomainError> {
        let _guard = self.db_access_guard()?;
        move_catalog::load_move(&self.state, move_id).await
    }

    pub async fn list_review_decisions(
        &self,
        match_id: Uuid,
        query: ReviewQuery,
    ) -> Result<ReviewDecisionPage, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(review_decisions::load_review(&self.state, match_id, query).await?)
    }

    pub async fn get_imported_analysis(
        &self,
        match_id: Uuid,
    ) -> Result<ImportedAnalysisSummary, DomainError> {
        let _guard = self.db_access_guard()?;
        imported_analysis::load_match_summary(&self.state, match_id).await
    }

    pub async fn get_position_imported_analysis(
        &self,
        position_id: Uuid,
    ) -> Result<Vec<ImportedAnalysisRecord>, DomainError> {
        let _guard = self.db_access_guard()?;
        imported_analysis::load_position_records(&self.state, position_id).await
    }

    pub async fn get_move_imported_analysis(
        &self,
        move_id: Uuid,
    ) -> Result<Vec<ImportedAnalysisRecord>, DomainError> {
        let _guard = self.db_access_guard()?;
        imported_analysis::load_move_records(&self.state, move_id).await
    }

    // ── Opening Book, Cube Context/Chances, Game Plans & Explanations ────────

    pub async fn lookup_opening_book_position(
        &self,
        position_id: Uuid,
    ) -> Result<BookLookup, DomainError> {
        let _guard = self.db_access_guard()?;
        opening_book::lookup_position_book(&self.state, position_id).await
    }

    /// Synchronous, database-free match equity & take-point context for a cube decision.
    pub fn get_cube_context(
        &self,
        match_id: &str,
        doubler: i32,
    ) -> Result<CubeContext, DomainError> {
        cube_context::cube_context(match_id, doubler).map_err(DomainError::invalid)
    }

    pub async fn calculate_cube_chances(
        &self,
        position: &Position,
        response: bool,
    ) -> Result<CubeChances, DomainError> {
        let _guard = self.db_access_guard()?;
        Ok(practice::evaluation::cube_chances(&self.state, position, response).await?)
    }

    /// Synchronous, database-free game-plan classification for both sides.
    pub fn classify_game_plans(&self, board: &[i32; 26]) -> Result<GamePlans, DomainError> {
        game_plan::classify(board).map_err(DomainError::invalid)
    }

    /// Synchronous, database-free deterministic move explanation.
    pub fn explain_move(
        &self,
        request: &MoveExplanationRequest,
    ) -> Result<MoveExplanation, DomainError> {
        move_explanation::explain(request).map_err(DomainError::invalid)
    }

    // ── Annotations & Tags ───────────────────────────────────────────────────

    pub async fn get_annotations(
        &self,
        kind: AnnotationKind,
        id: Uuid,
    ) -> Result<Annotation, DomainError> {
        let _guard = self.db_access_guard()?;
        annotations::load_target_annotation(&self.state, kind, id).await
    }

    pub async fn save_annotations(
        &self,
        kind: AnnotationKind,
        id: Uuid,
        update: AnnotationUpdate,
    ) -> Result<Annotation, DomainError> {
        let _guard = self.db_access_guard()?;
        annotations::save_target_annotation(&self.state, kind, id, update).await
    }

    pub async fn search_annotations(
        &self,
        query: SearchQuery,
    ) -> Result<AnnotationPage, DomainError> {
        let _guard = self.db_access_guard()?;
        annotations::run_search_annotations(&self.state, query).await
    }

    pub async fn list_tags(&self, query: TagQuery) -> Result<Vec<String>, DomainError> {
        let _guard = self.db_access_guard()?;
        annotations::list_annotation_tags(&self.state, query).await
    }

    pub async fn game_move_annotations(
        &self,
        match_id: Uuid,
        game_no: i32,
    ) -> Result<Vec<Annotation>, DomainError> {
        let _guard = self.db_access_guard()?;
        annotations::load_game_move_annotations(&self.state, match_id, game_no).await
    }
}
