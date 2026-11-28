//! Versioned local desktop UI state replacing browser `localStorage` and
//! `sessionStorage` keys (`neondojo.*`).
//!
//! Persisted atomically (write temp file → rename) to
//! `~/Library/Application Support/NeonDojo/ui-state.json` on macOS.

use crate::theme::{NeonPaletteId, ThemeMode};
use gpui_kit::Global;
use neondojo_rules::options::analysis_board_width_bounds;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const UI_STATE_SCHEMA_VERSION: u32 = 1;
const DEFAULT_LOCALE: &str = "en";
const DEFAULT_BOARD_THEME: &str = "forest";
const DEFAULT_EXTRAS_TOOL: &str = "benchmark";
const ALL_CARDS_DECK: &str = "__all__";
const MAX_COLUMN_WIDTH: u32 = 2400;

/// Cached appearance preference for zero-FOUC first frame startup
/// (`neondojo.ui-appearance`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedAppearance {
    #[serde(default)]
    pub theme: NeonPaletteId,
    #[serde(default)]
    pub mode: ThemeMode,
}

impl Default for CachedAppearance {
    fn default() -> Self {
        Self {
            theme: NeonPaletteId::Graphite,
            mode: ThemeMode::System,
        }
    }
}

/// Last opened match/game/position in Analysis (`neondojo.workspace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSelection {
    pub match_id: Uuid,
    pub game_no: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_id: Option<Uuid>,
}

/// Independent Analysis board pane widths per sub-tab (`320..=1680` px).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisBoardWidths {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub statistics: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<u32>,
}

impl AnalysisBoardWidths {
    pub fn clamp_all(&mut self) {
        let (min_w, max_w) = analysis_board_width_bounds();
        let min_w = min_w.max(0) as u32;
        let max_w = max_w.max(min_w as i32) as u32;
        for w in [
            &mut self.default_width,
            &mut self.overview,
            &mut self.errors,
            &mut self.statistics,
            &mut self.notes,
        ]
        .into_iter()
        .flatten()
        {
            *w = (*w).clamp(min_w, max_w);
        }
    }
}

/// Local Match Library table & bulk-analysis session preferences.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryUiPrefs {
    #[serde(default)]
    pub database_id: String,
    #[serde(default)]
    pub visible_columns: Vec<String>,
    #[serde(default)]
    pub column_order: Vec<String>,
    #[serde(default)]
    pub column_widths: BTreeMap<String, u32>,
    /// Map of tracked bulk-analysis job IDs to their database UUID (or `None` for all).
    #[serde(default)]
    pub bulk_analysis_jobs: BTreeMap<Uuid, Option<Uuid>>,
    #[serde(default)]
    pub show_match_list: bool,
}

/// Local Saved Positions / Decisions table preferences.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionsUiPrefs {
    #[serde(default)]
    pub visible_columns: Vec<String>,
    #[serde(default)]
    pub column_widths: BTreeMap<String, u32>,
    #[serde(default)]
    pub show_played: bool,
}

/// Pending similar-practice result queued for retry (`neondojo.similarPractice.pending.*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingSimilarPractice {
    pub key: String,
    pub payload_json: String,
}

/// Local Study & Flashcard session preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudyUiPrefs {
    #[serde(default = "default_deck_id")]
    pub deck_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub practice_round_id: Option<String>,
    #[serde(default)]
    pub independent_only: bool,
    #[serde(default)]
    pub pending_similar_practice: Vec<PendingSimilarPractice>,
}

impl Default for StudyUiPrefs {
    fn default() -> Self {
        Self {
            deck_id: default_deck_id(),
            practice_round_id: None,
            independent_only: false,
            pending_similar_practice: Vec::new(),
        }
    }
}

/// Unsynced profile edit recovery journal (`neondojo.configuration-pending.v1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingConfigJournal {
    pub base_revision: i64,
    pub configuration: Value,
}

/// Desktop bootstrap overrides (DB URL and external BGBlitz themes directory).
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootstrapPrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_themes_dir: Option<PathBuf>,
}

/// Authoritative local desktop UI state covering all 15 web local/session keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalUiState {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub appearance: CachedAppearance,
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default = "default_board_theme")]
    pub board_theme: String,
    #[serde(default)]
    pub archive_themes_preload: bool,
    #[serde(default = "default_true")]
    pub play_sounds: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_selection: Option<WorkspaceSelection>,
    #[serde(default)]
    pub analysis_board_widths: AnalysisBoardWidths,
    #[serde(default)]
    pub library: LibraryUiPrefs,
    #[serde(default)]
    pub positions: PositionsUiPrefs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_preview_width: Option<u32>,
    #[serde(default = "default_extras_tool")]
    pub extras_tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub play_session_id: Option<Uuid>,
    #[serde(default)]
    pub study: StudyUiPrefs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_configuration_journal: Option<PendingConfigJournal>,
    #[serde(default)]
    pub bootstrap: BootstrapPrefs,
}

impl Global for LocalUiState {}

impl Default for LocalUiState {
    fn default() -> Self {
        Self {
            schema_version: UI_STATE_SCHEMA_VERSION,
            appearance: CachedAppearance::default(),
            locale: default_locale(),
            board_theme: default_board_theme(),
            archive_themes_preload: false,
            play_sounds: true,
            workspace_selection: None,
            analysis_board_widths: AnalysisBoardWidths::default(),
            library: LibraryUiPrefs::default(),
            positions: PositionsUiPrefs::default(),
            card_preview_width: None,
            extras_tool: default_extras_tool(),
            play_session_id: None,
            study: StudyUiPrefs::default(),
            pending_configuration_journal: None,
            bootstrap: BootstrapPrefs::default(),
        }
    }
}

impl LocalUiState {
    /// Returns the NeonDojo application-support directory.
    ///
    /// On macOS this resolves to `~/Library/Application Support/NeonDojo`.
    pub fn app_support_dir() -> PathBuf {
        let base = dirs::data_local_dir()
            .or_else(dirs::config_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("NeonDojo")
    }

    /// Returns the default application-support path for `ui-state.json`.
    ///
    /// On macOS this resolves to `~/Library/Application Support/NeonDojo/ui-state.json`.
    pub fn default_path() -> PathBuf {
        Self::app_support_dir().join("ui-state.json")
    }

    /// Loads state from disk, falling back cleanly to defaults if missing or corrupt.
    pub fn load_or_default(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(mut state) = serde_json::from_str::<Self>(&raw) else {
            tracing::warn!(
                "corrupt local UI state at {}; using defaults",
                path.display()
            );
            return Self::default();
        };
        state.sanitize();
        state
    }

    /// Normalizes retired locales, invalid game numbers, and out-of-range widths.
    pub fn sanitize(&mut self) {
        self.schema_version = UI_STATE_SCHEMA_VERSION;
        self.locale = normalize_locale(&self.locale).to_string();
        if self.board_theme.trim().is_empty() {
            self.board_theme = default_board_theme();
        }
        if let Some(sel) = &self.workspace_selection
            && sel.game_no < 1
        {
            self.workspace_selection = None;
        }
        self.analysis_board_widths.clamp_all();
        for width in self.library.column_widths.values_mut() {
            *width = (*width).clamp(24, MAX_COLUMN_WIDTH);
        }
        for width in self.positions.column_widths.values_mut() {
            *width = (*width).clamp(24, MAX_COLUMN_WIDTH);
        }
        if let Some(width) = &mut self.card_preview_width {
            *width = (*width).clamp(240, 1680);
        }
        if !matches!(
            self.extras_tool.as_str(),
            "benchmark" | "duels" | "backup" | "versions"
        ) {
            self.extras_tool = default_extras_tool();
        }
        if self.study.deck_id.trim().is_empty() {
            self.study.deck_id = default_deck_id();
        }
    }

    /// Persists state atomically by writing a sibling temp file and renaming.
    pub fn save_atomic(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        let mut sanitized = self.clone();
        sanitized.sanitize();
        let bytes = serde_json::to_vec_pretty(&sanitized)?;
        let tmp_name = format!(
            ".{}.{}.tmp",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("ui-state.json"),
            Uuid::new_v4()
        );
        let tmp_path = path.with_file_name(tmp_name);
        {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, path)?;
        Ok(())
    }
}

/// Maps active locales (`en`, `de`) and migrates retired locales (`es`, `fr`, `it`, `pl`) to `en`.
pub fn normalize_locale(code: &str) -> &'static str {
    match code.trim().to_ascii_lowercase().as_str() {
        "de" => "de",
        _ => DEFAULT_LOCALE,
    }
}

fn default_schema_version() -> u32 {
    UI_STATE_SCHEMA_VERSION
}

fn default_locale() -> String {
    DEFAULT_LOCALE.to_string()
}

fn default_board_theme() -> String {
    DEFAULT_BOARD_THEME.to_string()
}

fn default_extras_tool() -> String {
    DEFAULT_EXTRAS_TOOL.to_string()
}

fn default_deck_id() -> String {
    ALL_CARDS_DECK.to_string()
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_atomic_persistence_and_sanitization() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("NeonDojo").join("ui-state.json");

        let match_id = Uuid::new_v4();
        let pos_id = Uuid::new_v4();
        let play_id = Uuid::new_v4();
        let job_id = Uuid::new_v4();
        let db_id = Uuid::new_v4();

        let mut state = LocalUiState {
            appearance: CachedAppearance {
                theme: NeonPaletteId::Club,
                mode: ThemeMode::Dark,
            },
            locale: "de".into(),
            board_theme: "bgblitz:cl_green".into(),
            archive_themes_preload: true,
            play_sounds: false,
            workspace_selection: Some(WorkspaceSelection {
                match_id,
                game_no: 3,
                position_id: Some(pos_id),
            }),
            analysis_board_widths: AnalysisBoardWidths {
                default_width: Some(120), // clamped to 320
                overview: Some(820),
                errors: Some(9999), // clamped to 1680
                statistics: None,
                notes: Some(640),
            },
            library: LibraryUiPrefs {
                database_id: db_id.to_string(),
                visible_columns: vec!["white".into(), "black".into()],
                column_order: vec!["white".into(), "black".into()],
                column_widths: BTreeMap::from([("white".into(), 220), ("huge".into(), 9000)]),
                bulk_analysis_jobs: BTreeMap::from([(job_id, Some(db_id))]),
                show_match_list: true,
            },
            positions: PositionsUiPrefs {
                visible_columns: vec!["origin".into(), "loss".into()],
                column_widths: BTreeMap::from([("origin".into(), 140)]),
                show_played: true,
            },
            card_preview_width: Some(500),
            extras_tool: "duels".into(),
            play_session_id: Some(play_id),
            study: StudyUiPrefs {
                deck_id: "deck-1".into(),
                practice_round_id: Some("round-9".into()),
                independent_only: true,
                pending_similar_practice: vec![PendingSimilarPractice {
                    key: "k1".into(),
                    payload_json: "{\"ok\":true}".into(),
                }],
            },
            pending_configuration_journal: Some(PendingConfigJournal {
                base_revision: 7,
                configuration: serde_json::json!({"schema_version": 1}),
            }),
            bootstrap: BootstrapPrefs {
                database_url: Some("postgres://127.0.0.1:54329/neondojo".into()),
                custom_themes_dir: Some(PathBuf::from("/tmp/custom-themes")),
            },
            ..LocalUiState::default()
        };

        state.save_atomic(&path).expect("atomic save");
        let loaded = LocalUiState::load_or_default(&path);

        assert_eq!(loaded.appearance.theme, NeonPaletteId::Club);
        assert_eq!(loaded.appearance.mode, ThemeMode::Dark);
        assert_eq!(loaded.locale, "de");
        assert_eq!(loaded.analysis_board_widths.default_width, Some(320));
        assert_eq!(loaded.analysis_board_widths.overview, Some(820));
        assert_eq!(loaded.analysis_board_widths.errors, Some(1680));
        assert_eq!(loaded.library.column_widths.get("huge"), Some(&2400));
        assert_eq!(loaded.play_session_id, Some(play_id));
        assert!(loaded.study.independent_only);
        assert_eq!(loaded.study.pending_similar_practice.len(), 1);
        assert_eq!(
            loaded.bootstrap.database_url.as_deref(),
            Some("postgres://127.0.0.1:54329/neondojo")
        );

        // Retired locale falls back cleanly to English.
        state.locale = "fr".into();
        state.save_atomic(&path).expect("save retired locale");
        let migrated = LocalUiState::load_or_default(&path);
        assert_eq!(migrated.locale, "en");
    }

    #[test]
    fn corrupt_or_missing_file_returns_default() {
        let dir = tempdir().expect("tempdir");
        let missing = dir.path().join("missing.json");
        assert_eq!(
            LocalUiState::load_or_default(&missing),
            LocalUiState::default()
        );

        let corrupt = dir.path().join("corrupt.json");
        fs::write(&corrupt, "{ not valid json").expect("write corrupt");
        assert_eq!(
            LocalUiState::load_or_default(&corrupt),
            LocalUiState::default()
        );
    }
}
