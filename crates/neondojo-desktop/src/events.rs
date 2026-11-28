//! Cross-workspace typed event bus replacing React prop drilling and stringified revision counters.

use crate::settings::draft_guard::SettingsCategory;
use crate::shell::AppNotice;
use gpui_kit::SharedString;
use uuid::Uuid;

/// Events emitted by workspaces (`EventEmitter<WorkspaceEvent>`) and routed by [`crate::shell::AppShell`].
#[derive(Debug, Clone)]
pub enum WorkspaceEvent {
    /// Open a match (and optional game/position/cube decision) in the Analysis workspace.
    OpenMatchInAnalysis {
        match_id: Uuid,
        game_no: Option<i32>,
        position_id: Option<Uuid>,
        decision_no: Option<i32>,
    },
    /// Open or resume a live/paused play session in the Play workspace.
    OpenPlaySession { session_id: Uuid, resume: bool },
    /// Open the specified database in the Match Library.
    OpenLibraryDatabase { database_id: Uuid },
    /// Jump to Match Library with an exact GNUbg Position ID filter applied.
    SearchPositionInLibrary { position_id: String },
    /// Start bulk match analysis in Match Library for the given match IDs.
    StartBulkAnalysis { match_ids: Vec<Uuid> },
    /// Open a specific Extras sub-tool (`benchmark`, `matches`, `backup`, `versions`).
    OpenExtrasTool { tool: SharedString },
    /// Open the Settings modal on a specific category.
    OpenSettings { category: SettingsCategory },
    /// Open Study reports for a specific match.
    OpenStudyReport { match_id: Uuid },
    /// Notify listeners that library matches/databases changed.
    LibraryMutated,
    /// Notify listeners that an analysis job started, progressed, completed, or failed.
    AnalysisJobChanged { match_id: Option<Uuid> },
    /// Notify Analysis to invalidate cached data after Play switches to Competition mode.
    PlaySwitchedToCompetition { match_id: Uuid },
    /// Notify listeners that annotations were saved for a match.
    AnnotationSaved { match_id: Uuid },
    /// Request a full desktop entity reset (for example after restoring a database backup).
    TriggerAppReset { reason: SharedString },
    /// Surface a banner notice in [`crate::shell::AppShell`]'s `AppNotices` strip.
    ShowNotice(AppNotice),
    /// Toggle the global benchmark-busy banner below the header.
    SetBenchmarkBusy(bool),
}
