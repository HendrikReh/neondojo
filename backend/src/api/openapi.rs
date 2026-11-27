use super::{ErrorBody, Health, VersionInfo, analysis, jobs, matches};
use crate::engine::{Analysis, Candidate, DiceEvaluation, Distribution};
use crate::json_errors::Json;
use crate::model::*;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "NeonDojo API",
        description = "Local headless backgammon library with GNUbg and NeonGammon analysis. No authentication; bind to loopback. Requests whose Host is not a loopback name, and unsafe requests from a foreign Origin, receive HTTP 403."
    ),
    paths(
        super::health,
        super::version,
        matches::upload,
        matches::matches,
        matches::get_match,
        matches::match_games,
        matches::match_import_warnings,
        matches::source,
        matches::positions,
        matches::get_position,
        matches::dice,
        jobs::create_job,
        jobs::job,
        jobs::cancel_job,
        jobs::job_status_batch,
        jobs::events,
        analysis::create_match_analysis,
        analysis::latest_match_analysis,
        analysis::match_analysis_jobs,
        analysis::move_analyses
    ),
    components(schemas(
        Health,
        VersionInfo,
        ErrorBody,
        matches::UploadForm,
        ImportResponse,
        ImportWarning,
        ImportWarningKind,
        MatchSummary,
        MatchGame,
        MatchPage,
        Position,
        PositionPage,
        jobs::JobRequest,
        Job,
        jobs::JobStatusRequest,
        jobs::JobStatusBatch,
        JobEvent,
        JobProgress,
        DiceStats,
        DiceCount,
        Analysis,
        Candidate,
        Distribution,
        DiceEvaluation,
        analysis::MatchAnalysisRequest,
        MatchAnalysisSummary,
        PlayerAnalysisSummary,
        MoveAnalysis,
        MoveAnalysisPage,
        MoveSeverity,
        crate::match_analysis::Decision
    ))
)]
pub struct BaseApiDoc;

pub struct ApiDoc;
impl OpenApi for ApiDoc {
    fn openapi() -> utoipa::openapi::OpenApi {
        let mut doc = BaseApiDoc::openapi();
        doc.merge(crate::settings::SettingsDoc::openapi());
        doc.merge(crate::settings::open_sage::OpenSageDoc::openapi());
        doc.merge(crate::configuration::ConfigurationDoc::openapi());
        doc.merge(crate::profiles::ProfilesDoc::openapi());
        doc.merge(crate::benchmarks::BenchmarkDoc::openapi());
        doc.merge(crate::engine_matches::EngineMatchDoc::openapi());
        doc.merge(crate::benchmarks::estimate::EstimateDoc::openapi());
        doc.merge(crate::analysis_timing::TimingDoc::openapi());
        doc.merge(crate::play::PlayDoc::openapi());
        doc.merge(crate::study::StudyDoc::openapi());
        doc.merge(crate::match_assessment::AssessmentDoc::openapi());
        doc.merge(crate::recurring_mistakes::RecurringMistakesDoc::openapi());
        doc.merge(crate::opening_book::OpeningBookDoc::openapi());
        doc.merge(crate::imported_analysis::ImportedAnalysisDoc::openapi());
        doc.merge(crate::analysis_catalog::AnalysisCatalogDoc::openapi());
        doc.merge(crate::databases::DatabaseDoc::openapi());
        doc.merge(crate::duplicate_matches::DuplicateMatchesDoc::openapi());
        doc.merge(crate::database_backup::BackupDoc::openapi());
        doc.merge(crate::player_directory::openapi());
        doc.merge(crate::move_catalog::MoveCatalogDoc::openapi());
        doc.merge(crate::annotations::AnnotationDoc::openapi());
        doc.merge(crate::learning::LearningDoc::openapi());
        doc.merge(crate::learning::daily_doc());
        doc.merge(crate::analysis_statistics::StatisticsDoc::openapi());
        doc.merge(crate::analysis_statistics::catalog::CatalogStatisticsDoc::openapi());
        doc.merge(crate::position_browser::PositionBrowserDoc::openapi());
        doc.merge(crate::similar_positions::SimilarPositionsDoc::openapi());
        doc.merge(crate::similar_positions::SimilarPracticeHistoryDoc::openapi());
        doc.merge(crate::position_bookmarks::PositionBookmarksDoc::openapi());
        doc.merge(crate::move_assessments::MoveAssessmentsDoc::openapi());
        doc.merge(crate::review_decisions::ReviewDoc::openapi());
        doc.merge(crate::game_plan::GamePlanDoc::openapi());
        doc.merge(crate::cube_context::CubeContextDoc::openapi());
        doc.merge(crate::move_explanation::MoveExplanationDoc::openapi());
        doc
    }
}

pub(super) async fn openapi() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}
