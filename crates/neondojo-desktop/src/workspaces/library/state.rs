//! Pure domain state, column layout preferences, selection caps, filter
//! mapping, bulk-analysis progress tracking, and formatting helpers for the
//! native GPUI Match Library workspace.

use crate::events::WorkspaceEvent;
use crate::local_state::LibraryUiPrefs;
use crate::ui::filter_chips::AppliedFilterChip;
use chrono::{DateTime, NaiveDate, Utc};
use gpui_kit::SharedString;
use neondojo::annotations::{AnnotationHit, AnnotationKind, SearchQuery as AnnotationSearchQuery};
use neondojo::api::JobStatusBatch;
use neondojo::databases::{
    ActionResponse, Database, DatabaseMatch, LibraryOccurrence, LibraryQuery,
};
use neondojo::duplicate_matches::DuplicateMatchGroup;
use neondojo::model::{ImportWarning, ImportWarningKind, Job};
use neondojo::play::{BOT_DATABASE_ID, PAUSED_MATCHES_DATABASE_ID, PlayMode};
use neondojo_i18n::{I18n, Locale, Param};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

/// System database UUID for Engine Duels (`backend/src/engine_matches/runner.rs`).
pub const ENGINE_DUELS_DATABASE_ID: Uuid = Uuid::from_u128(5);

/// Maximum number of matches selectable across pages for batch operations.
pub const MAX_SELECTION: usize = 100;
/// Maximum number of matches allowed in a single export action.
pub const MAX_EXPORT_SELECTION: usize = 50;
/// Maximum combined export payload size enforced by the backend (64 MiB).
pub const MAX_EXPORT_BYTES: usize = 64 * 1024 * 1024;
/// Default page size for the match table.
pub const PAGE_SIZE: usize = 50;
/// Default page size for annotation / notes search.
pub const NOTES_PAGE_SIZE: usize = 20;
/// Default group page size for the cursor-paged duplicate match finder.
pub const DUPLICATES_PAGE_SIZE: usize = 50;
/// Maximum clamped column width in logical pixels (mirrors `LocalUiState`).
pub const MAX_COLUMN_WIDTH: u32 = 2400;

/// Succeeded batch item statuses returned by `BackendFacade::apply_library_action`.
const SUCCEEDED_ACTION_STATUSES: &[&str] = &[
    "copied",
    "moved",
    "deleted",
    "tagged",
    "same_database",
    "duplicate",
];

/// Explicit clone helper for `neondojo::databases::Database` (which does not derive `Clone`).
pub fn clone_database(db: &Database) -> Database {
    Database {
        id: db.id,
        name: db.name.clone(),
        description: db.description.clone(),
        version: db.version,
        is_default: db.is_default,
        is_system: db.is_system,
        created_at: db.created_at,
        updated_at: db.updated_at,
        match_count: db.match_count,
    }
}

/// Explicit clone helper for `neondojo::databases::LibraryOccurrence`.
pub fn clone_occurrence(occ: &LibraryOccurrence) -> LibraryOccurrence {
    LibraryOccurrence {
        id: occ.id,
        game_no: occ.game_no,
        move_no: occ.move_no,
        database_name: occ.database_name.clone(),
    }
}

/// Explicit clone helper for `neondojo::databases::DatabaseMatch` (which does not derive `Clone`).
pub fn clone_database_match(m: &DatabaseMatch) -> DatabaseMatch {
    DatabaseMatch {
        occurrence: m.occurrence.as_ref().map(clone_occurrence),
        player_sides: m.player_sides.clone(),
        opponent_player_id: m.opponent_player_id,
        id: m.id,
        source_available: m.source_available,
        play_session_id: m.play_session_id,
        play_paused: m.play_paused,
        study_report_id: m.study_report_id,
        play_phase: m.play_phase.clone(),
        play_mode: m.play_mode,
        play_assisted: m.play_assisted,
        match_no: m.match_no,
        filename: m.filename.clone(),
        format: m.format.clone(),
        white: m.white.clone(),
        black: m.black.clone(),
        match_length: m.match_length,
        position_count: m.position_count,
        created_at: m.created_at,
        database_id: m.database_id,
        display_name: m.display_name.clone(),
        played_on: m.played_on,
        tournament: m.tournament.clone(),
        round: m.round.clone(),
        place: m.place.clone(),
        result_state: m.result_state.clone(),
        white_score: m.white_score,
        black_score: m.black_score,
        white_pr: m.white_pr,
        black_pr: m.black_pr,
        pr_method: m.pr_method.clone(),
        pr_profile: m.pr_profile.clone(),
        analysis_status: m.analysis_status.clone(),
        game_count: m.game_count,
        import_warning_count: m.import_warning_count,
        tags: m.tags.clone(),
        comment: m.comment.clone(),
        version: m.version,
    }
}

/// Validates a 14-character GNUbg Position ID (`/^[A-Za-z0-9+/]{14}$/`).
pub fn is_valid_position_id(raw: &str) -> bool {
    let trimmed = raw.trim();
    trimmed.len() == 14
        && trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
}

/// Returns true if a database may receive imported, copied, or moved matches.
pub fn accepts_library_matches(database_id: Uuid) -> bool {
    database_id != PAUSED_MATCHES_DATABASE_ID
}

/// Returns true if a database can be emptied (`Empty database` action on non-paused system DBs).
pub fn can_empty_database(db: &Database) -> bool {
    db.is_system && db.id != PAUSED_MATCHES_DATABASE_ID && db.match_count > 0
}

/// Returns true if a database can be permanently deleted (user databases only).
pub fn can_delete_database(db: &Database) -> bool {
    !db.is_system
}

/// Localized display name for a database (localizes the protected `Paused Matches` DB).
pub fn database_display_name(db: &Database, locale: Locale) -> String {
    if db.id == PAUSED_MATCHES_DATABASE_ID {
        I18n::t(locale, "library", "pausedMatches").to_string()
    } else {
        db.name.clone()
    }
}

/// Builds the sidebar tooltip for a database matching `DatabaseSidebar.tsx`.
pub fn database_tooltip(db: &Database, locale: Locale) -> Option<String> {
    let description = db.description.trim();
    let repeats_name = description.eq_ignore_ascii_case(db.name.trim());
    let default_description = if db.id == PAUSED_MATCHES_DATABASE_ID {
        I18n::t(locale, "library", "pausedMatchesDescription")
    } else if db.id == BOT_DATABASE_ID {
        I18n::t(locale, "library", "botMatchesDescription")
    } else {
        ""
    };
    let effective_desc = if !description.is_empty() && !repeats_name {
        description
    } else {
        default_description
    };

    let mut parts = Vec::new();
    if db.is_system {
        parts.push(I18n::t(locale, "library", "systemDatabase"));
    }
    if !effective_desc.is_empty() {
        parts.push(effective_desc);
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

/// Returns true when a match represents an unfinished Competition play session.
///
/// Mirrors `library-match-summary.ts::isUnfinishedCompetition`.
pub fn is_unfinished_competition(m: &DatabaseMatch) -> bool {
    m.play_mode == Some(PlayMode::Competition)
        && m.play_session_id.is_some()
        && m.play_phase.as_deref() != Some("match_over")
        && !(m.match_length == 0 && m.play_phase.as_deref() == Some("game_over"))
}

/// Returns true if a new whole-match analysis job can be submitted for `m`.
pub fn can_analyze_match(m: &DatabaseMatch) -> bool {
    m.source_available && !m.play_paused && !is_unfinished_competition(m)
}

/// Returns true if PR values may be displayed for `m`.
pub fn can_show_match_pr(m: &DatabaseMatch) -> bool {
    !m.play_assisted
        && (m.play_mode.is_none() || m.play_mode == Some(PlayMode::Competition))
        && !is_unfinished_competition(m)
}

/// Returns true if the PR column should show a warning badge (partial analysis or import warnings).
pub fn has_pr_warning(m: &DatabaseMatch) -> bool {
    m.import_warning_count > 0 || m.analysis_status.as_deref() == Some("partial")
}

/// Returns the resumable play session ID when the single-match "Resume match" button applies.
pub fn resumable_session_id(m: &DatabaseMatch) -> Option<Uuid> {
    let session_id = m.play_session_id?;
    if m.play_paused
        || (m.play_phase.as_deref() != Some("match_over")
            && !(m.match_length == 0 && m.play_phase.as_deref() == Some("game_over")))
    {
        Some(session_id)
    } else {
        None
    }
}

/// Authoritative row-click routing invariant for Match Library:
///
/// Paused matches and unfinished Competition rows **MUST** emit
/// [`WorkspaceEvent::OpenPlaySession`] with `resume: true` (never Analysis).
/// All other rows open in Analysis, preserving occurrence context when present.
pub fn row_open_event(m: &DatabaseMatch) -> WorkspaceEvent {
    if (m.play_paused || is_unfinished_competition(m))
        && let Some(session_id) = m.play_session_id
    {
        return WorkspaceEvent::OpenPlaySession {
            session_id,
            resume: true,
        };
    }

    WorkspaceEvent::OpenMatchInAnalysis {
        match_id: m.id,
        game_no: m.occurrence.as_ref().map(|o| o.game_no),
        position_id: m.occurrence.as_ref().map(|o| o.id),
        decision_no: None,
    }
}

/// Unique row key distinguishing multiple position occurrences within one match.
pub fn library_row_key(m: &DatabaseMatch) -> String {
    match &m.occurrence {
        Some(occ) => format!("{}:{}", m.id, occ.id),
        None => m.id.to_string(),
    }
}

/// Localized analysis status label matching `libraryAnalysisStatus`.
pub fn format_library_analysis_status(status: Option<&str>, locale: Locale) -> &'static str {
    let key = match status {
        Some("imported") => "importedFile",
        Some("completed") => "completed",
        Some("queued") => "queued",
        Some("running") => "running",
        Some("failed") => "failed",
        Some("partial") => "partial",
        _ => "noAnalysis",
    };
    I18n::t(locale, "library", key)
}

/// Localized play-mode & assistance badge label.
pub fn format_play_mode(mode: PlayMode, assisted: bool, locale: Locale) -> String {
    let mode_key = match mode {
        PlayMode::Competition => "competition",
        PlayMode::Training => "training",
        PlayMode::Coach => "coach",
        PlayMode::Tutor => "tutor",
    };
    let assist_key = if assisted { "assisted" } else { "independent" };
    format!(
        "{} · {}",
        I18n::t(locale, "play-mode", mode_key),
        I18n::t(locale, "play-mode", assist_key)
    )
}

/// Formats a numeric PR with two decimal places using the locale's decimal separator.
pub fn format_pr(value: Option<f64>, locale: Locale) -> String {
    match value {
        Some(v) if v.is_finite() => {
            let formatted = format!("{v:.2}");
            if locale.decimal_separator() == ',' {
                formatted.replace('.', ",")
            } else {
                formatted
            }
        }
        _ => "—".to_string(),
    }
}

/// Formats a match date (`YYYY-MM-DD`) cleanly for dense table display.
pub fn format_naive_date(date: Option<NaiveDate>) -> String {
    match date {
        Some(d) => d.format("%Y-%m-%d").to_string(),
        None => "—".to_string(),
    }
}

/// Formats an imported timestamp (`YYYY-MM-DD`) cleanly for dense table display.
pub fn format_utc_date(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d").to_string()
}

/// Splits a comma-separated tag input into trimmed, non-empty, deduplicated tags.
pub fn parse_tag_csv(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for piece in raw.split(',') {
        let trimmed = piece.trim();
        if !trimmed.is_empty() && seen.insert(trimmed.to_lowercase()) {
            out.push(trimmed.to_string());
        }
    }
    out
}

/// Formats an [`ImportWarning`] into localized user-facing copy.
pub fn format_import_warning(warning: &ImportWarning, locale: Locale) -> String {
    let game = warning.game_no.to_string();
    let ply = warning.source_ply.unwrap_or(1).to_string();
    let line = warning.source_line.unwrap_or(1).to_string();
    let text = warning.source_text.as_deref().unwrap_or("");
    match warning.kind {
        ImportWarningKind::IllegalPlayReplaced => I18n::format(
            locale,
            "library",
            "importWarningIllegalPlay",
            &[
                ("game", Param::from(game.as_str())),
                ("ply", Param::from(ply.as_str())),
            ],
        ),
        ImportWarningKind::CustomStartPosition => I18n::format(
            locale,
            "library",
            "importWarningCustomStart",
            &[("game", Param::from(game.as_str()))],
        ),
        ImportWarningKind::UnfinishedGame => I18n::format(
            locale,
            "library",
            "importWarningUnfinishedGame",
            &[("game", Param::from(game.as_str()))],
        ),
        ImportWarningKind::IllegalPlayTruncated => I18n::format(
            locale,
            "library",
            "importWarningIllegalTruncated",
            &[
                ("game", Param::from(game.as_str())),
                ("ply", Param::from(ply.as_str())),
                ("line", Param::from(line.as_str())),
                ("text", Param::from(text)),
            ],
        ),
        ImportWarningKind::UnreadablePlayTruncated => I18n::format(
            locale,
            "library",
            "importWarningUnreadableTruncated",
            &[
                ("game", Param::from(game.as_str())),
                ("ply", Param::from(ply.as_str())),
                ("line", Param::from(line.as_str())),
                ("text", Param::from(text)),
            ],
        ),
        ImportWarningKind::ScoreMismatch => {
            let recorded = warning
                .recorded_score
                .map(|s| format!("{}–{}", s[0], s[1]))
                .unwrap_or_else(|| "—".into());
            let expected = warning
                .expected_score
                .map(|s| format!("{}–{}", s[0], s[1]))
                .unwrap_or_else(|| "—".into());
            I18n::format(
                locale,
                "library",
                "importWarningScoreMismatch",
                &[
                    ("game", Param::from(game.as_str())),
                    ("line", Param::from(line.as_str())),
                    ("recorded", Param::from(recorded.as_str())),
                    ("expected", Param::from(expected.as_str())),
                ],
            )
        }
    }
}

// ── 15 Toggleable / Reorderable / Resizable Table Columns ────────────────────

/// All 15 toggleable data columns in the Match Library table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LibraryDataColumn {
    Id,
    White,
    WhitePr,
    Black,
    BlackPr,
    PlayMode,
    MatchLength,
    Score,
    PlayedAt,
    CreatedAt,
    Tournament,
    PositionCount,
    AnalysisStatus,
    Tags,
    Comment,
}

impl LibraryDataColumn {
    /// Canonical order of all 15 data columns (matching `library-columns.ts`).
    pub const ALL: &'static [Self] = &[
        Self::Id,
        Self::White,
        Self::WhitePr,
        Self::Black,
        Self::BlackPr,
        Self::PlayMode,
        Self::MatchLength,
        Self::Score,
        Self::PlayedAt,
        Self::CreatedAt,
        Self::Tournament,
        Self::PositionCount,
        Self::AnalysisStatus,
        Self::Tags,
        Self::Comment,
    ];

    /// Default visible columns (all 15 except `Tournament` and `Comment`).
    pub const DEFAULT_VISIBLE: &'static [Self] = &[
        Self::Id,
        Self::White,
        Self::WhitePr,
        Self::Black,
        Self::BlackPr,
        Self::PlayMode,
        Self::MatchLength,
        Self::Score,
        Self::PlayedAt,
        Self::CreatedAt,
        Self::PositionCount,
        Self::AnalysisStatus,
        Self::Tags,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::White => "white",
            Self::WhitePr => "white_pr",
            Self::Black => "black",
            Self::BlackPr => "black_pr",
            Self::PlayMode => "play_mode",
            Self::MatchLength => "match_length",
            Self::Score => "score",
            Self::PlayedAt => "played_at",
            Self::CreatedAt => "created_at",
            Self::Tournament => "tournament",
            Self::PositionCount => "position_count",
            Self::AnalysisStatus => "analysis_status",
            Self::Tags => "tags",
            Self::Comment => "comment",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.id() == id)
    }

    pub const fn min_width(self) -> u32 {
        match self {
            Self::Id | Self::WhitePr | Self::BlackPr | Self::MatchLength => 44,
            Self::Score => 54,
            Self::White | Self::Black => 60,
            Self::PlayedAt
            | Self::CreatedAt
            | Self::Tournament
            | Self::PositionCount
            | Self::AnalysisStatus => 70,
            Self::PlayMode => 90,
            Self::Tags | Self::Comment => 96,
        }
    }

    pub const fn default_width(self) -> u32 {
        match self {
            Self::Id => 60,
            Self::White | Self::Black => 180,
            Self::WhitePr | Self::BlackPr => 56,
            Self::PlayMode => 210,
            Self::MatchLength => 86,
            Self::Score => 76,
            Self::PlayedAt | Self::CreatedAt => 100,
            Self::Tournament => 140,
            Self::PositionCount => 96,
            Self::AnalysisStatus => 126,
            Self::Tags => 160,
            Self::Comment => 220,
        }
    }

    /// Server-side `LibraryQuery::sort` key, or `None` if unsortable (`play_mode`).
    pub const fn sort_key(self) -> Option<&'static str> {
        match self {
            Self::PlayMode => None,
            _ => Some(self.id()),
        }
    }

    /// Compact header label shown in table column headers.
    pub fn header_label(self, locale: Locale) -> String {
        match self {
            Self::Id => "ID".to_string(),
            Self::White => I18n::t(locale, "library", "white").to_string(),
            Self::WhitePr | Self::BlackPr => "PR".to_string(),
            Self::Black => I18n::t(locale, "library", "black").to_string(),
            Self::PlayMode => I18n::t(locale, "play-mode", "mode").to_string(),
            Self::MatchLength => I18n::t(locale, "library", "length").to_string(),
            Self::Score => I18n::t(locale, "library", "result").to_string(),
            Self::PlayedAt => I18n::t(locale, "library", "played").to_string(),
            Self::CreatedAt => I18n::t(locale, "library", "imported").to_string(),
            Self::Tournament => I18n::t(locale, "library", "tournament").to_string(),
            Self::PositionCount => I18n::t(locale, "workspace", "positions").to_string(),
            Self::AnalysisStatus => I18n::t(locale, "library", "analysis").to_string(),
            Self::Tags => I18n::t(locale, "library", "tags").to_string(),
            Self::Comment => I18n::t(locale, "library", "comment").to_string(),
        }
    }

    /// Descriptive column name used in the column picker, tooltips, and hidden-sort badge.
    pub fn picker_label(self, locale: Locale) -> String {
        match self {
            Self::WhitePr => format!("{} · PR", I18n::t(locale, "library", "white")),
            Self::BlackPr => format!("{} · PR", I18n::t(locale, "library", "black")),
            _ => self.header_label(locale),
        }
    }
}

/// The 3 dynamic columns prepended when a GNUbg Position ID search is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OccurrenceColumn {
    Database,
    Game,
    Move,
}

impl OccurrenceColumn {
    pub const ALL: &'static [Self] = &[Self::Database, Self::Game, Self::Move];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Database => "occurrence_database",
            Self::Game => "occurrence_game",
            Self::Move => "occurrence_move",
        }
    }

    pub const fn sort_key(self) -> &'static str {
        self.id()
    }

    pub const fn min_width(self) -> u32 {
        match self {
            Self::Database => 70,
            Self::Game | Self::Move => 44,
        }
    }

    pub const fn default_width(self) -> u32 {
        match self {
            Self::Database => 160,
            Self::Game | Self::Move => 70,
        }
    }

    pub fn label(self, locale: Locale) -> &'static str {
        match self {
            Self::Database => I18n::t(locale, "library", "database"),
            Self::Game => I18n::t(locale, "library", "game"),
            Self::Move => I18n::t(locale, "library", "sourceMove"),
        }
    }
}

/// Manages visibility, ordering, and widths of the 15 library columns + 3 occurrence columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnLayoutState {
    visible: BTreeSet<LibraryDataColumn>,
    order: Vec<LibraryDataColumn>,
    widths: BTreeMap<String, u32>,
}

impl Default for ColumnLayoutState {
    fn default() -> Self {
        Self {
            visible: LibraryDataColumn::DEFAULT_VISIBLE.iter().copied().collect(),
            order: LibraryDataColumn::ALL.to_vec(),
            widths: BTreeMap::new(),
        }
    }
}

impl ColumnLayoutState {
    /// Restores column layout from persisted [`LibraryUiPrefs`], enforcing
    /// canonical fallbacks and minimum-1 visibility.
    pub fn from_prefs(prefs: &LibraryUiPrefs) -> Self {
        let mut visible: BTreeSet<LibraryDataColumn> = prefs
            .visible_columns
            .iter()
            .filter_map(|id| LibraryDataColumn::from_id(id))
            .collect();
        if visible.is_empty() {
            visible = LibraryDataColumn::DEFAULT_VISIBLE.iter().copied().collect();
        }

        let mut order = Vec::with_capacity(LibraryDataColumn::ALL.len());
        let mut seen = BTreeSet::new();
        for id in &prefs.column_order {
            if let Some(col) = LibraryDataColumn::from_id(id)
                && seen.insert(col)
            {
                order.push(col);
            }
        }
        for &col in LibraryDataColumn::ALL {
            if seen.insert(col) {
                order.push(col);
            }
        }

        let mut widths = BTreeMap::new();
        for (key, &w) in &prefs.column_widths {
            let min = LibraryDataColumn::from_id(key)
                .map(LibraryDataColumn::min_width)
                .or_else(|| {
                    OccurrenceColumn::ALL
                        .iter()
                        .find(|c| c.id() == key)
                        .map(|c| c.min_width())
                })
                .unwrap_or(24);
            widths.insert(key.clone(), w.clamp(min, MAX_COLUMN_WIDTH));
        }

        Self {
            visible,
            order,
            widths,
        }
    }

    /// Writes column layout back into [`LibraryUiPrefs`].
    pub fn write_to_prefs(&self, prefs: &mut LibraryUiPrefs) {
        prefs.visible_columns = self
            .ordered_visible()
            .into_iter()
            .map(|c| c.id().to_string())
            .collect();
        prefs.column_order = self.order.iter().map(|c| c.id().to_string()).collect();
        prefs.column_widths = self.widths.clone();
    }

    pub fn order(&self) -> &[LibraryDataColumn] {
        &self.order
    }

    pub fn is_visible(&self, col: LibraryDataColumn) -> bool {
        self.visible.contains(&col)
    }

    /// Returns visible columns in the user's chosen order.
    pub fn ordered_visible(&self) -> Vec<LibraryDataColumn> {
        self.order
            .iter()
            .copied()
            .filter(|c| self.visible.contains(c))
            .collect()
    }

    /// Toggles visibility of a column, enforcing that at least 1 data column remains visible.
    pub fn toggle_visible(&mut self, col: LibraryDataColumn, checked: bool) -> bool {
        if checked {
            self.visible.insert(col)
        } else if self.visible.len() <= 1 && self.visible.contains(&col) {
            false
        } else {
            self.visible.remove(&col)
        }
    }

    /// Moves a column left (`delta = -1`) or right (`delta = 1`) in the column order.
    pub fn move_column(&mut self, col: LibraryDataColumn, delta: isize) -> bool {
        let Some(idx) = self.order.iter().position(|&c| c == col) else {
            return false;
        };
        let target = idx as isize + delta;
        if target < 0 || target >= self.order.len() as isize {
            return false;
        }
        self.order.swap(idx, target as usize);
        true
    }

    pub fn show_all(&mut self) {
        self.visible = LibraryDataColumn::ALL.iter().copied().collect();
    }

    pub fn reset_defaults(&mut self) {
        self.visible = LibraryDataColumn::DEFAULT_VISIBLE.iter().copied().collect();
        self.order = LibraryDataColumn::ALL.to_vec();
    }

    pub fn reset_widths(&mut self) {
        self.widths.clear();
    }

    pub fn has_custom_widths(&self) -> bool {
        !self.widths.is_empty()
    }

    pub fn width_of(&self, col: LibraryDataColumn) -> u32 {
        self.widths
            .get(col.id())
            .copied()
            .unwrap_or_else(|| col.default_width())
            .clamp(col.min_width(), MAX_COLUMN_WIDTH)
    }

    pub fn width_of_occurrence(&self, col: OccurrenceColumn) -> u32 {
        self.widths
            .get(col.id())
            .copied()
            .unwrap_or_else(|| col.default_width())
            .clamp(col.min_width(), MAX_COLUMN_WIDTH)
    }

    pub fn set_width(&mut self, col_id: &str, min_width: u32, width: u32) {
        self.widths
            .insert(col_id.to_string(), width.clamp(min_width, MAX_COLUMN_WIDTH));
    }

    pub fn adjust_width(&mut self, col: LibraryDataColumn, delta: i32) {
        let current = self.width_of(col) as i32;
        let next = (current + delta).max(col.min_width() as i32) as u32;
        self.set_width(col.id(), col.min_width(), next);
    }

    pub fn adjust_occurrence_width(&mut self, col: OccurrenceColumn, delta: i32) {
        let current = self.width_of_occurrence(col) as i32;
        let next = (current + delta).max(col.min_width() as i32) as u32;
        self.set_width(col.id(), col.min_width(), next);
    }

    /// Returns true if the active sort key targets a currently hidden data column.
    pub fn is_sort_column_hidden(&self, sort: &str, has_occurrences: bool) -> bool {
        if has_occurrences && OccurrenceColumn::ALL.iter().any(|c| c.sort_key() == sort) {
            return false;
        }
        match LibraryDataColumn::from_id(sort) {
            Some(col) => !self.visible.contains(&col),
            None => false,
        }
    }

    /// Human-readable label for a sort key (used by the hidden-sort badge).
    pub fn sort_label(sort: &str, locale: Locale) -> String {
        if let Some(occ) = OccurrenceColumn::ALL.iter().find(|c| c.sort_key() == sort) {
            return occ.label(locale).to_string();
        }
        if let Some(col) = LibraryDataColumn::from_id(sort) {
            return col.picker_label(locale);
        }
        sort.to_string()
    }
}

// ── Mutually Exclusive Accordion Search Panels ───────────────────────────────

/// The 4 mutually-exclusive search/tool panels above the library table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchPanelKind {
    Filters,
    Notes,
    Position,
    Duplicates,
}

/// Toggles an accordion panel: clicking the active panel collapses it; clicking
/// another panel switches directly to it while preserving all panel drafts.
pub fn toggle_search_panel(current: &mut Option<SearchPanelKind>, target: SearchPanelKind) {
    if *current == Some(target) {
        *current = None;
    } else {
        *current = Some(target);
    }
}

// ── Sort & Filter State ──────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    #[default]
    Desc,
}

impl SortDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }

    pub const fn toggle(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }

    pub fn label(self, locale: Locale) -> &'static str {
        match self {
            Self::Asc => I18n::t(locale, "library", "ascending"),
            Self::Desc => I18n::t(locale, "library", "descending"),
        }
    }
}

/// Complete library search & filter criteria mapped onto [`LibraryQuery`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryFilterState {
    pub q: String,
    pub player: String,
    pub match_name: String,
    pub tournament: String,
    pub round: String,
    pub date_field: String,
    pub date_from: String,
    pub date_to: String,
    pub tag: String,
    pub comment: String,
    pub match_type: String,
    pub result: String,
    pub match_points_min: String,
    pub match_points_max: String,
    /// Validated 14-character GNUbg Position ID (when Position search is active).
    pub position_id: String,
}

impl Default for LibraryFilterState {
    fn default() -> Self {
        Self {
            q: String::new(),
            player: String::new(),
            match_name: String::new(),
            tournament: String::new(),
            round: String::new(),
            date_field: "played".to_string(),
            date_from: String::new(),
            date_to: String::new(),
            tag: String::new(),
            comment: String::new(),
            match_type: String::new(),
            result: String::new(),
            match_points_min: String::new(),
            match_points_max: String::new(),
            position_id: String::new(),
        }
    }
}

fn non_empty_trimmed(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

impl LibraryFilterState {
    pub fn has_position_filter(&self) -> bool {
        is_valid_position_id(&self.position_id)
    }

    pub fn has_active_filters(&self) -> bool {
        [
            &self.q,
            &self.player,
            &self.match_name,
            &self.tournament,
            &self.round,
            &self.date_from,
            &self.date_to,
            &self.tag,
            &self.comment,
            &self.match_type,
            &self.result,
            &self.match_points_min,
            &self.match_points_max,
            &self.position_id,
        ]
        .iter()
        .any(|v| !v.trim().is_empty())
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Removes a single applied filter chip by its chip ID.
    pub fn remove_filter(&mut self, chip_id: &str) -> bool {
        match chip_id {
            "q" => self.q.clear(),
            "player" => self.player.clear(),
            "match_name" => self.match_name.clear(),
            "tournament" => self.tournament.clear(),
            "round" => self.round.clear(),
            "date_from" => self.date_from.clear(),
            "date_to" => self.date_to.clear(),
            "tag" => self.tag.clear(),
            "comment" => self.comment.clear(),
            "match_type" => self.match_type.clear(),
            "result" => self.result.clear(),
            "match_points_min" => self.match_points_min.clear(),
            "match_points_max" => self.match_points_max.clear(),
            "position_id" => self.position_id.clear(),
            _ => return false,
        }
        true
    }

    /// Builds the backend [`LibraryQuery`] for the active database, sort, and 1-based page.
    pub fn to_library_query(
        &self,
        database_id: Option<Uuid>,
        sort: &str,
        direction: SortDirection,
        page: usize,
    ) -> LibraryQuery {
        let valid_pos = non_empty_trimmed(&self.position_id).filter(|p| is_valid_position_id(p));
        let effective_sort =
            if valid_pos.is_none() && OccurrenceColumn::ALL.iter().any(|c| c.sort_key() == sort) {
                "created_at"
            } else {
                sort
            };

        let date_from = non_empty_trimmed(&self.date_from)
            .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());
        let date_to = non_empty_trimmed(&self.date_to)
            .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());
        let date_field = if date_from.is_some() || date_to.is_some() {
            Some(if self.date_field == "imported" {
                "imported".to_string()
            } else {
                "played".to_string()
            })
        } else {
            None
        };

        LibraryQuery {
            database_id,
            q: non_empty_trimmed(&self.q),
            player: non_empty_trimmed(&self.player),
            player_id: None,
            virtual_player_id: None,
            date_from,
            date_to,
            date_field,
            tag: non_empty_trimmed(&self.tag),
            comment: non_empty_trimmed(&self.comment),
            position_id: valid_pos,
            tournament: non_empty_trimmed(&self.tournament),
            round: non_empty_trimmed(&self.round),
            match_name: non_empty_trimmed(&self.match_name),
            match_type: non_empty_trimmed(&self.match_type),
            result: non_empty_trimmed(&self.result),
            match_points_min: non_empty_trimmed(&self.match_points_min)
                .and_then(|s| s.parse::<i32>().ok()),
            match_points_max: non_empty_trimmed(&self.match_points_max)
                .and_then(|s| s.parse::<i32>().ok()),
            sort: Some(effective_sort.to_string()),
            direction: Some(direction.as_str().to_string()),
            after: None,
            page: Some(page.max(1) as i64),
            limit: Some(PAGE_SIZE as i64),
        }
    }

    /// Generates removable [`AppliedFilterChip`] pills for all active filters.
    pub fn applied_chips(&self, locale: Locale) -> Vec<AppliedFilterChip> {
        let mut chips = Vec::new();
        let date_prefix = if self.date_field == "imported" {
            I18n::t(locale, "library", "imported")
        } else {
            I18n::t(locale, "library", "played")
        };

        let mut push = |id: &'static str, label: String, value: &str| {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                chips.push(AppliedFilterChip::new(id, label).value(trimmed.to_string()));
            }
        };

        push(
            "q",
            I18n::t(locale, "position-browser", "q").to_string(),
            &self.q,
        );
        push(
            "player",
            I18n::t(locale, "library", "player").to_string(),
            &self.player,
        );
        push(
            "match_name",
            I18n::t(locale, "library", "matchName").to_string(),
            &self.match_name,
        );
        push(
            "tournament",
            I18n::t(locale, "library", "tournament").to_string(),
            &self.tournament,
        );
        push(
            "round",
            I18n::t(locale, "library", "round").to_string(),
            &self.round,
        );
        push(
            "date_from",
            format!("{date_prefix} · {}", I18n::t(locale, "library", "dateFrom")),
            &self.date_from,
        );
        push(
            "date_to",
            format!("{date_prefix} · {}", I18n::t(locale, "library", "dateTo")),
            &self.date_to,
        );
        push(
            "tag",
            I18n::t(locale, "library", "tag").to_string(),
            &self.tag,
        );
        push(
            "comment",
            I18n::t(locale, "library", "comment").to_string(),
            &self.comment,
        );
        if let Some(mt) = non_empty_trimmed(&self.match_type) {
            let val = match mt.as_str() {
                "match" => I18n::t(locale, "library", "match"),
                "money" => I18n::t(locale, "library", "money"),
                other => other,
            };
            push(
                "match_type",
                I18n::t(locale, "library", "type").to_string(),
                val,
            );
        }
        if let Some(res) = non_empty_trimmed(&self.result) {
            let val = match res.as_str() {
                "completed" => I18n::t(locale, "library", "completed"),
                "white_won" => I18n::t(locale, "library", "whiteWon"),
                "black_won" => I18n::t(locale, "library", "blackWon"),
                "draw" => I18n::t(locale, "library", "draw"),
                "ongoing" => I18n::t(locale, "library", "ongoing"),
                "unknown" => I18n::t(locale, "library", "unknown"),
                other => other,
            };
            push(
                "result",
                I18n::t(locale, "library", "result").to_string(),
                val,
            );
        }
        push(
            "match_points_min",
            I18n::t(locale, "library", "minPoints").to_string(),
            &self.match_points_min,
        );
        push(
            "match_points_max",
            I18n::t(locale, "library", "maxPoints").to_string(),
            &self.match_points_max,
        );
        push(
            "position_id",
            I18n::t(locale, "app", "app.positionId").to_string(),
            &self.position_id,
        );

        chips
    }
}

// ── Cross-Page Deduped Selection (Cap 100 / Export Cap 50) ───────────────────

/// Summary of a batch action response, distinguishing succeeded vs retained failed IDs.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BatchActionSummary {
    pub succeeded: usize,
    pub failed: usize,
    pub status_counts: BTreeMap<String, usize>,
    pub errors: Vec<String>,
}

impl BatchActionSummary {
    pub fn has_failures(&self) -> bool {
        self.failed > 0
    }

    pub fn format_message(&self, locale: Locale) -> String {
        let mut parts = Vec::new();
        for (status, count) in &self.status_counts {
            let label = match status.as_str() {
                "copied" => I18n::t(locale, "library", "copied"),
                "moved" => I18n::t(locale, "library", "moved"),
                "deleted" => I18n::t(locale, "library", "deleted"),
                "tagged" => I18n::t(locale, "library", "tagged"),
                "duplicate" => I18n::t(locale, "library", "duplicate"),
                "same_database" => I18n::t(locale, "library", "sameDatabase"),
                "not_found" => I18n::t(locale, "library", "notFound"),
                "busy" => I18n::t(locale, "library", "busy"),
                _ => I18n::t(locale, "library", "error"),
            };
            parts.push(format!("{label}: {count}"));
        }
        if !self.errors.is_empty() {
            parts.push(self.errors.join("; "));
        }
        parts.join(" · ")
    }
}

/// Cross-page deduplicated match selection enforcing the 100-match cap,
/// 50-match export cap, paused-match restrictions, and partial-failure retention.
#[derive(Debug, Default)]
pub struct LibrarySelectionState {
    items: Vec<DatabaseMatch>,
}

impl LibrarySelectionState {
    pub fn items(&self) -> &[DatabaseMatch] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn is_at_cap(&self) -> bool {
        self.items.len() >= MAX_SELECTION
    }

    pub fn contains(&self, id: Uuid) -> bool {
        self.items.iter().any(|m| m.id == id)
    }

    pub fn ids(&self) -> Vec<Uuid> {
        self.items.iter().map(|m| m.id).collect()
    }

    pub fn single(&self) -> Option<&DatabaseMatch> {
        if self.items.len() == 1 {
            self.items.first()
        } else {
            None
        }
    }

    pub fn has_paused(&self) -> bool {
        self.items.iter().any(|m| m.play_paused)
    }

    /// True when the current selection is non-empty and within the 50-match export cap.
    pub fn can_export(&self) -> bool {
        !self.items.is_empty() && self.items.len() <= MAX_EXPORT_SELECTION
    }

    /// True when copy/move/delete are permitted for the current selection.
    pub fn can_mutate_membership(&self) -> bool {
        !self.items.is_empty() && !self.has_paused()
    }

    /// Returns `(analyzable_matches, unavailable_count)` for the current selection.
    pub fn partition_analyzable(&self) -> (Vec<DatabaseMatch>, usize) {
        let analyzable: Vec<DatabaseMatch> = self
            .items
            .iter()
            .filter(|m| can_analyze_match(m))
            .map(clone_database_match)
            .collect();
        let unavailable = self.items.len().saturating_sub(analyzable.len());
        (analyzable, unavailable)
    }

    /// Adds or removes matches while deduplicating by `match.id` and capping at [`MAX_SELECTION`].
    pub fn set_many(&mut self, matches: &[DatabaseMatch], checked: bool) {
        if checked {
            for m in matches {
                if let Some(existing) = self.items.iter_mut().find(|x| x.id == m.id) {
                    *existing = clone_database_match(m);
                } else if self.items.len() < MAX_SELECTION {
                    self.items.push(clone_database_match(m));
                }
            }
        } else {
            let remove_ids: BTreeSet<Uuid> = matches.iter().map(|m| m.id).collect();
            self.items.retain(|m| !remove_ids.contains(&m.id));
        }
    }

    /// Refreshes cached match metadata for selected items that appear in the latest page.
    pub fn sync_visible_page(&mut self, page_items: &[DatabaseMatch]) {
        for selected in &mut self.items {
            if let Some(fresh) = page_items.iter().find(|m| m.id == selected.id) {
                *selected = clone_database_match(fresh);
            }
        }
    }

    /// Selects every duplicate copy except the oldest entry (index 0) in each listed group,
    /// respecting the 100-match cap. Returns how many matches were added.
    pub fn select_duplicate_copies(&mut self, groups: &[DuplicateMatchGroup]) -> usize {
        let before = self.items.len();
        for group in groups {
            for copy in group.items.iter().skip(1) {
                if self.items.len() >= MAX_SELECTION {
                    break;
                }
                if !self.contains(copy.id) {
                    self.items.push(clone_database_match(copy));
                }
            }
            if self.items.len() >= MAX_SELECTION {
                break;
            }
        }
        self.items.len().saturating_sub(before)
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Evaluates a backend [`ActionResponse`], keeping ONLY failed match IDs selected
    /// (partial batch failure retention) and returning a structured summary.
    pub fn retain_failed(&mut self, response: &ActionResponse) -> BatchActionSummary {
        let mut summary = BatchActionSummary::default();
        let mut failed_ids = BTreeSet::new();

        for item in &response.results {
            *summary
                .status_counts
                .entry(item.status.clone())
                .or_insert(0) += 1;
            if SUCCEEDED_ACTION_STATUSES.contains(&item.status.as_str()) {
                summary.succeeded += 1;
            } else {
                summary.failed += 1;
                failed_ids.insert(item.match_id);
                if let Some(err) = &item.error
                    && !err.trim().is_empty()
                    && !summary.errors.contains(err)
                {
                    summary.errors.push(err.clone());
                }
            }
        }

        if failed_ids.is_empty() {
            self.items.clear();
        } else {
            self.items.retain(|m| failed_ids.contains(&m.id));
        }
        summary
    }
}

// ── Bulk Analysis Progress Tracking ──────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedBulkJob {
    pub job_id: Uuid,
    pub database_id: Option<Uuid>,
    pub match_id: Option<Uuid>,
    pub status: String,
    pub progress: i32,
    pub completed_games: i32,
    pub total_games: i32,
}

impl TrackedBulkJob {
    pub fn is_terminal(&self) -> bool {
        !matches!(self.status.as_str(), "queued" | "running")
    }
}

/// Tracks queued/running bulk-analysis jobs across refreshes and persists active IDs
/// in `LocalUiState::library.bulk_analysis_jobs`.
#[derive(Debug, Default, Clone)]
pub struct BulkAnalysisTracker {
    jobs: BTreeMap<Uuid, TrackedBulkJob>,
}

impl BulkAnalysisTracker {
    pub fn from_prefs(prefs: &LibraryUiPrefs) -> Self {
        let mut jobs = BTreeMap::new();
        for (&job_id, &database_id) in &prefs.bulk_analysis_jobs {
            jobs.insert(
                job_id,
                TrackedBulkJob {
                    job_id,
                    database_id,
                    match_id: None,
                    status: "queued".to_string(),
                    progress: 0,
                    completed_games: 0,
                    total_games: 0,
                },
            );
        }
        Self { jobs }
    }

    pub fn write_to_prefs(&self, prefs: &mut LibraryUiPrefs) {
        prefs.bulk_analysis_jobs = self
            .jobs
            .values()
            .filter(|j| !j.is_terminal())
            .map(|j| (j.job_id, j.database_id))
            .collect();
    }

    pub fn track_jobs(&mut self, submitted: &[Job], database_id: Option<Uuid>) {
        for job in submitted {
            self.jobs.insert(
                job.id,
                TrackedBulkJob {
                    job_id: job.id,
                    database_id,
                    match_id: job.match_uuid,
                    status: job.status.clone(),
                    progress: job.progress,
                    completed_games: job.completed_games,
                    total_games: job.total_games,
                },
            );
        }
    }

    pub fn active_job_ids(&self) -> Vec<Uuid> {
        self.jobs
            .values()
            .filter(|j| !j.is_terminal())
            .map(|j| j.job_id)
            .collect()
    }

    pub fn has_active_jobs(&self) -> bool {
        self.jobs.values().any(|j| !j.is_terminal())
    }

    /// Applies a single job status transition; returns `(changed, became_terminal)`.
    pub fn apply_job(&mut self, job: &Job) -> (bool, bool) {
        let Some(entry) = self.jobs.get_mut(&job.id) else {
            return (false, false);
        };
        let was_terminal = entry.is_terminal();
        let changed = entry.status != job.status
            || entry.progress != job.progress
            || entry.completed_games != job.completed_games
            || entry.total_games != job.total_games;
        entry.match_id = job.match_uuid;
        entry.status = job.status.clone();
        entry.progress = job.progress;
        entry.completed_games = job.completed_games;
        entry.total_games = job.total_games;
        let became_terminal = !was_terminal && entry.is_terminal();
        (changed, became_terminal)
    }

    /// Applies a [`JobStatusBatch`] snapshot; returns `(any_changed, completed_match_ids)`.
    pub fn apply_batch(&mut self, batch: &JobStatusBatch) -> (bool, Vec<Option<Uuid>>) {
        let mut any_changed = false;
        let mut finished_matches = Vec::new();
        for job in &batch.jobs {
            let (changed, became_terminal) = self.apply_job(job);
            if changed {
                any_changed = true;
            }
            if became_terminal {
                finished_matches.push(job.match_uuid);
            }
        }
        for missing_id in batch.missing.iter().chain(batch.restricted.iter()) {
            if self.jobs.remove(missing_id).is_some() {
                any_changed = true;
            }
        }
        if !self.has_active_jobs() && !self.jobs.is_empty() {
            // Keep completed batch summary visible briefly until next mutation or clear.
        }
        (any_changed, finished_matches)
    }

    /// Returns `(completed_count, total_count)` for the active scope, if any jobs exist.
    pub fn scope_progress(&self, database_id: Option<Uuid>) -> Option<(usize, usize)> {
        let relevant: Vec<&TrackedBulkJob> = self
            .jobs
            .values()
            .filter(|j| database_id.is_none() || j.database_id == database_id)
            .collect();
        if relevant.is_empty() {
            return None;
        }
        let completed = relevant.iter().filter(|j| j.is_terminal()).count();
        Some((completed, relevant.len()))
    }

    pub fn dismiss_if_finished(&mut self) {
        if !self.has_active_jobs() {
            self.jobs.clear();
        }
    }
}

// ── Notes Search & Duplicate Finder States ───────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct NotesSearchState {
    pub query: String,
    pub exact_tag: String,
    pub kind: Option<AnnotationKind>,
    pub page: usize,
    pub loading: bool,
    pub searched: bool,
    pub error: Option<String>,
    pub hits: Vec<AnnotationHit>,
    pub total: Option<i64>,
}

impl NotesSearchState {
    pub fn new() -> Self {
        Self {
            page: 1,
            ..Self::default()
        }
    }

    /// Validates annotation search input before hitting the backend (3–200 chars or exact tag).
    pub fn validate(&self, locale: Locale) -> Result<(), String> {
        let q = self.query.trim();
        let tag = self.exact_tag.trim();
        if q.is_empty() && tag.is_empty() {
            return Err(I18n::t(locale, "annotation", "searchRequired").to_string());
        }
        if !q.is_empty() {
            let char_len = q.chars().count();
            if char_len < 3 {
                return Err(I18n::t(locale, "annotation", "queryTooShort").to_string());
            }
            if char_len > 200 {
                return Err(I18n::t(locale, "annotation", "queryTooLong").to_string());
            }
            if q.chars().any(|c| c.is_control()) {
                return Err(I18n::t(locale, "annotation", "invalidQuery").to_string());
            }
        }
        if tag.chars().any(|c| c.is_control()) {
            return Err(I18n::t(locale, "annotation", "invalidTag").to_string());
        }
        Ok(())
    }

    pub fn to_query(&self, database_id: Option<Uuid>) -> AnnotationSearchQuery {
        AnnotationSearchQuery {
            q: non_empty_trimmed(&self.query),
            tag: non_empty_trimmed(&self.exact_tag),
            kind: self.kind,
            after: None,
            page: Some(self.page.max(1) as i64),
            limit: Some(NOTES_PAGE_SIZE as i64),
            database_id,
        }
    }
}

#[derive(Debug, Default)]
pub struct DuplicateFinderState {
    pub groups: Vec<DuplicateMatchGroup>,
    pub next_cursor: Option<String>,
    pub total_groups: i64,
    pub total_matches: i64,
    pub loading: bool,
    pub loading_more: bool,
    pub searched: bool,
    pub error: Option<String>,
}

impl DuplicateFinderState {
    pub fn clear(&mut self) {
        self.groups.clear();
        self.next_cursor = None;
        self.total_groups = 0;
        self.total_matches = 0;
        self.searched = false;
        self.error = None;
    }
}

// ── Native Import Run State ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportFileStatus {
    Imported {
        match_id: Uuid,
        position_count: i32,
        warnings: Vec<ImportWarning>,
    },
    Duplicate {
        match_id: Uuid,
        warnings: Vec<ImportWarning>,
    },
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct ImportFileRow {
    pub path: PathBuf,
    pub display_name: String,
    pub status: ImportFileStatus,
}

#[derive(Debug, Clone)]
pub struct ImportRunState {
    pub running: bool,
    pub stopping: bool,
    pub stopped: bool,
    pub total: usize,
    pub done: usize,
    pub imported: usize,
    pub duplicates: usize,
    pub failed: usize,
    pub skipped_unsupported: usize,
    pub ignored_hidden: usize,
    pub destination_name: SharedString,
    pub rows: Vec<ImportFileRow>,
    pub error: Option<String>,
    pub cancel_flag: Arc<AtomicBool>,
}

impl ImportRunState {
    pub fn start(
        total: usize,
        skipped_unsupported: usize,
        ignored_hidden: usize,
        destination_name: impl Into<SharedString>,
    ) -> Self {
        Self {
            running: total > 0,
            stopping: false,
            stopped: false,
            total,
            done: 0,
            imported: 0,
            duplicates: 0,
            failed: 0,
            skipped_unsupported,
            ignored_hidden,
            destination_name: destination_name.into(),
            rows: Vec::with_capacity(total.min(256)),
            error: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn request_stop(&mut self) {
        if self.running {
            self.stopping = true;
            self.cancel_flag.store(true, Ordering::SeqCst);
        }
    }

    pub fn record_row(&mut self, row: ImportFileRow) {
        self.done += 1;
        match &row.status {
            ImportFileStatus::Imported { .. } => self.imported += 1,
            ImportFileStatus::Duplicate { .. } => self.duplicates += 1,
            ImportFileStatus::Failed(_) => self.failed += 1,
        }
        self.rows.push(row);
    }

    pub fn finish(&mut self, was_stopped: bool) {
        self.running = false;
        self.stopping = false;
        self.stopped = was_stopped;
    }

    pub fn total_warning_count(&self) -> usize {
        self.rows
            .iter()
            .map(|r| match &r.status {
                ImportFileStatus::Imported { warnings, .. }
                | ImportFileStatus::Duplicate { warnings, .. } => warnings.len(),
                ImportFileStatus::Failed(_) => 0,
            })
            .sum()
    }
}
