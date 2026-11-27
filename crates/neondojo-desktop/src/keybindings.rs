//! Global keyboard actions, `KeyContext` predicates, key bindings, and native
//! macOS menu bar construction for NeonDojo.

use crate::theme::{NeonPaletteId, ThemeMode};
use gpui_kit::{App, KeyBinding, Menu, MenuItem, OsAction, SharedString, SystemMenuType, actions};
use neondojo_i18n::{I18n, Locale};

/// Key context identifiers matching NeonDojo's focus hierarchy.
pub const CTX_APP: &str = "NeonApp";
pub const CTX_PLAY: &str = "NeonPlay";
pub const CTX_ANALYSIS: &str = "NeonAnalysis";
pub const CTX_STUDY_EXERCISE: &str = "NeonStudyExercise";
pub const CTX_BOARD_REVIEW: &str = "NeonBoardReview";
pub const CTX_MODAL: &str = "NeonModal";
pub const CTX_TABLE: &str = "NeonTable";
pub const CTX_TEXT_NAV: &str = "NeonTextNav";
pub const CTX_SPLITTER: &str = "NeonSplitter";
pub const CTX_TEXT_INPUT: &str = "Input";

actions!(
    neondojo,
    [
        // Application & Shell
        Quit,
        OpenSettings,
        OpenAbout,
        GoHome,
        SelectWorkspacePlay,
        SelectWorkspaceAnalysis,
        SelectWorkspaceStudy,
        SelectWorkspacePositions,
        SelectWorkspaceLibrary,
        SelectWorkspacePlayers,
        SelectWorkspaceExtras,
        // File & Library
        ImportMatchFiles,
        ImportMatchFolder,
        NewPlayMatch,
        ExportCurrentMatch,
        OpenDatabaseBackup,
        // Standard macOS Edit actions
        Undo,
        Redo,
        Cut,
        Copy,
        Paste,
        SelectAll,
        CopyPositionId,
        // View & Appearance
        ResetPaneWidths,
        SetPaletteGraphite,
        SetPaletteClub,
        SetPalettePaper,
        SetModeLight,
        SetModeDark,
        SetModeSystem,
        // Profile
        ShowProfileChanges,
        ManageProfiles,
        // Analysis
        PrevPosition,
        NextPosition,
        NextError,
        PrevError,
        AnalyzeGame,
        AnalyzeMatch,
        ToggleDiceEquity,
        // Board review (higher priority than Analysis position arrows)
        ShowInitialBoard,
        ShowFinalBoard,
        // Playing
        PrimaryPlayStep,
        OfferDouble,
        RequestHint,
        UndoCheckerStep,
        ReturnToLiveGame,
        // Study & Practice
        CubeNoDouble,
        CubeDouble,
        CubeTake,
        CubeDrop,
        ShowAnswer,
        CheckOrContinue,
        RateAgain,
        RateHard,
        RateGood,
        RateEasy,
        // Shared UI controls
        CloseModal,
        NavPrev,
        NavNext,
        NavFirst,
        NavLast,
        SplitterShrink,
        SplitterGrow,
        SplitterShrinkFast,
        SplitterGrowFast,
        SplitterMin,
        SplitterMax,
        SplitterReset,
        // Help
        OpenUserGuide,
        OpenNeonGammonManual,
    ]
);

/// Registers all global, workspace, review, modal, and control keybindings.
pub fn register_keybindings(cx: &mut App) {
    let play_ctx = Some("NeonPlay && !Input && !NeonModal");
    let analysis_ctx = Some("NeonAnalysis && !Input && !NeonModal");
    let study_ctx = Some("NeonStudyExercise && !Input");
    let review_ctx = Some("NeonBoardReview && !Input && !NeonModal");
    let modal_ctx = Some(CTX_MODAL);
    let nav_ctx = Some(CTX_TEXT_NAV);
    let splitter_ctx = Some(CTX_SPLITTER);

    cx.bind_keys([
        // Global app & navigation shortcuts (macOS standard Cmd modifiers)
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-0", GoHome, None),
        KeyBinding::new("cmd-1", SelectWorkspacePlay, None),
        KeyBinding::new("cmd-2", SelectWorkspaceAnalysis, None),
        KeyBinding::new("cmd-3", SelectWorkspaceStudy, None),
        KeyBinding::new("cmd-4", SelectWorkspacePositions, None),
        KeyBinding::new("cmd-5", SelectWorkspaceLibrary, None),
        KeyBinding::new("cmd-6", SelectWorkspacePlayers, None),
        KeyBinding::new("cmd-7", SelectWorkspaceExtras, None),
        KeyBinding::new("cmd-o", ImportMatchFiles, None),
        KeyBinding::new("cmd-shift-o", ImportMatchFolder, None),
        KeyBinding::new("cmd-n", NewPlayMatch, None),
        KeyBinding::new("cmd-shift-c", CopyPositionId, None),
        // Analysis shortcuts (outside text inputs and modals)
        KeyBinding::new("left", PrevPosition, analysis_ctx),
        KeyBinding::new("right", NextPosition, analysis_ctx),
        KeyBinding::new("e", NextError, analysis_ctx),
        KeyBinding::new("shift-e", PrevError, analysis_ctx),
        // Board review overrides Left/Right while previewing a move/roll
        KeyBinding::new("left", ShowInitialBoard, review_ctx),
        KeyBinding::new("right", ShowFinalBoard, review_ctx),
        // Playing shortcuts
        KeyBinding::new("enter", PrimaryPlayStep, play_ctx),
        KeyBinding::new("d", OfferDouble, play_ctx),
        KeyBinding::new("h", RequestHint, play_ctx),
        KeyBinding::new("z", UndoCheckerStep, play_ctx),
        KeyBinding::new("escape", ReturnToLiveGame, play_ctx),
        // Study & Practice shortcuts (work both inline and inside practice modal)
        KeyBinding::new("n", CubeNoDouble, study_ctx),
        KeyBinding::new("d", CubeDouble, study_ctx),
        KeyBinding::new("t", CubeTake, study_ctx),
        KeyBinding::new("p", CubeDrop, study_ctx),
        KeyBinding::new("z", UndoCheckerStep, study_ctx),
        KeyBinding::new("a", ShowAnswer, study_ctx),
        KeyBinding::new("enter", CheckOrContinue, study_ctx),
        KeyBinding::new("1", RateAgain, study_ctx),
        KeyBinding::new("2", RateHard, study_ctx),
        KeyBinding::new("3", RateGood, study_ctx),
        KeyBinding::new("4", RateEasy, study_ctx),
        // Modal escape
        KeyBinding::new("escape", CloseModal, modal_ctx),
        // Roving text navigation
        KeyBinding::new("left", NavPrev, nav_ctx),
        KeyBinding::new("right", NavNext, nav_ctx),
        KeyBinding::new("up", NavPrev, nav_ctx),
        KeyBinding::new("down", NavNext, nav_ctx),
        KeyBinding::new("home", NavFirst, nav_ctx),
        KeyBinding::new("end", NavLast, nav_ctx),
        // Focusable pane splitters
        KeyBinding::new("left", SplitterShrink, splitter_ctx),
        KeyBinding::new("right", SplitterGrow, splitter_ctx),
        KeyBinding::new("shift-left", SplitterShrinkFast, splitter_ctx),
        KeyBinding::new("shift-right", SplitterGrowFast, splitter_ctx),
        KeyBinding::new("home", SplitterMin, splitter_ctx),
        KeyBinding::new("end", SplitterMax, splitter_ctx),
        KeyBinding::new("enter", SplitterReset, splitter_ctx),
    ]);
}

/// Snapshot used to populate checkmarks and dynamic localized labels in the macOS menu bar.
#[derive(Debug, Clone)]
pub struct MenuState {
    pub locale: Locale,
    pub active_profile_name: SharedString,
    pub profile_modified: bool,
    pub palette: NeonPaletteId,
    pub mode: ThemeMode,
    pub active_workspace_index: Option<usize>,
    pub has_open_match: bool,
}

impl Default for MenuState {
    fn default() -> Self {
        Self {
            locale: Locale::DEFAULT,
            active_profile_name: SharedString::from("Standard"),
            profile_modified: false,
            palette: NeonPaletteId::Graphite,
            mode: ThemeMode::System,
            active_workspace_index: None,
            has_open_match: false,
        }
    }
}

/// Builds the full native macOS menu bar (`NeonDojo`, `File`, `Edit`, `View`,
/// `Profile`, `Analysis`, `Window`, `Help`) localized to `state.locale`.
pub fn build_app_menus(state: &MenuState) -> Vec<Menu> {
    let l = state.locale;
    let profile_header = if state.profile_modified {
        format!(
            "{}: {} · {}",
            I18n::t(l, "profile", "active"),
            state.active_profile_name,
            I18n::t(l, "profile", "modified")
        )
    } else {
        format!(
            "{}: {}",
            I18n::t(l, "profile", "active"),
            state.active_profile_name
        )
    };

    vec![
        Menu {
            name: "NeonDojo".into(),
            items: vec![
                MenuItem::action("About NeonDojo", OpenAbout),
                MenuItem::separator(),
                MenuItem::action(
                    format!("{}…", I18n::t(l, "settings", "title")),
                    OpenSettings,
                ),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit NeonDojo", Quit),
            ],
            disabled: false,
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action(
                    format!("{}…", I18n::t(l, "app", "app.importPlain")),
                    ImportMatchFiles,
                ),
                MenuItem::action("Import Match Folder…", ImportMatchFolder),
                MenuItem::separator(),
                MenuItem::action(
                    format!("{}…", I18n::t(l, "workspace", "playComputer")),
                    NewPlayMatch,
                ),
                MenuItem::separator(),
                MenuItem::action("Export Current Match…", ExportCurrentMatch)
                    .disabled(!state.has_open_match),
                MenuItem::action("Database Backup…", OpenDatabaseBackup),
            ],
            disabled: false,
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Undo", Undo, OsAction::Undo),
                MenuItem::os_action("Redo", Redo, OsAction::Redo),
                MenuItem::separator(),
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
                MenuItem::separator(),
                MenuItem::action(I18n::t(l, "app", "app.positionId"), CopyPositionId)
                    .disabled(!state.has_open_match),
            ],
            disabled: false,
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action(I18n::t(l, "workspace", "homeTitle"), GoHome)
                    .checked(state.active_workspace_index.is_none()),
                MenuItem::separator(),
                MenuItem::action(I18n::t(l, "workspace", "play"), SelectWorkspacePlay)
                    .checked(state.active_workspace_index == Some(0)),
                MenuItem::action(I18n::t(l, "workspace", "analysis"), SelectWorkspaceAnalysis)
                    .checked(state.active_workspace_index == Some(1)),
                MenuItem::action(I18n::t(l, "workspace", "learning"), SelectWorkspaceStudy)
                    .checked(state.active_workspace_index == Some(2)),
                MenuItem::action(I18n::t(l, "workspace", "saved"), SelectWorkspacePositions)
                    .checked(state.active_workspace_index == Some(3)),
                MenuItem::action(I18n::t(l, "workspace", "database"), SelectWorkspaceLibrary)
                    .checked(state.active_workspace_index == Some(4)),
                MenuItem::action(I18n::t(l, "workspace", "players"), SelectWorkspacePlayers)
                    .checked(state.active_workspace_index == Some(5)),
                MenuItem::action(I18n::t(l, "workspace", "extras"), SelectWorkspaceExtras)
                    .checked(state.active_workspace_index == Some(6)),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: I18n::t(l, "ui-appearance", "appearance").into(),
                    items: vec![
                        MenuItem::action(
                            I18n::t(l, "ui-appearance", "graphite"),
                            SetPaletteGraphite,
                        )
                        .checked(state.palette == NeonPaletteId::Graphite),
                        MenuItem::action(I18n::t(l, "ui-appearance", "club"), SetPaletteClub)
                            .checked(state.palette == NeonPaletteId::Club),
                        MenuItem::action(I18n::t(l, "ui-appearance", "paper"), SetPalettePaper)
                            .checked(state.palette == NeonPaletteId::Paper),
                        MenuItem::separator(),
                        MenuItem::action(I18n::t(l, "ui-appearance", "light"), SetModeLight)
                            .checked(state.mode == ThemeMode::Light),
                        MenuItem::action(I18n::t(l, "ui-appearance", "dark"), SetModeDark)
                            .checked(state.mode == ThemeMode::Dark),
                        MenuItem::action(I18n::t(l, "ui-appearance", "system"), SetModeSystem)
                            .checked(state.mode == ThemeMode::System),
                    ],
                    disabled: false,
                }),
                MenuItem::action("Reset Pane Widths", ResetPaneWidths),
            ],
            disabled: false,
        },
        Menu {
            name: I18n::t(l, "profile", "profile").into(),
            items: vec![
                MenuItem::action(profile_header, ManageProfiles).checked(true),
                MenuItem::separator(),
                MenuItem::action(
                    format!("{}…", I18n::t(l, "profile", "showChanges")),
                    ShowProfileChanges,
                )
                .disabled(!state.profile_modified),
                MenuItem::action(
                    format!("{}…", I18n::t(l, "profile", "manage")),
                    ManageProfiles,
                ),
            ],
            disabled: false,
        },
        Menu {
            name: I18n::t(l, "workspace", "analysis").into(),
            items: vec![
                MenuItem::action(I18n::t(l, "app", "gameAnalysis.start"), AnalyzeGame)
                    .disabled(!state.has_open_match),
                MenuItem::action(
                    format!("{}…", I18n::t(l, "app", "gameAnalysis.scopeMatch")),
                    AnalyzeMatch,
                )
                .disabled(!state.has_open_match),
                MenuItem::separator(),
                MenuItem::action(I18n::t(l, "app", "app.previousMove"), PrevPosition)
                    .disabled(!state.has_open_match),
                MenuItem::action(I18n::t(l, "app", "app.nextMove"), NextPosition)
                    .disabled(!state.has_open_match),
                MenuItem::action(I18n::t(l, "board-review", "previousError"), PrevError)
                    .disabled(!state.has_open_match),
                MenuItem::action(I18n::t(l, "workspace", "nextError"), NextError)
                    .disabled(!state.has_open_match),
                MenuItem::separator(),
                MenuItem::action("Dice Equity", ToggleDiceEquity).disabled(!state.has_open_match),
            ],
            disabled: false,
        },
        Menu {
            name: "Window".into(),
            items: vec![MenuItem::action(I18n::t(l, "settings", "done"), CloseModal)],
            disabled: false,
        },
        Menu {
            name: I18n::t(l, "app", "app.help").into(),
            items: vec![
                MenuItem::action(
                    format!("{} ↗", I18n::t(l, "app", "app.neondojoManual")),
                    OpenUserGuide,
                ),
                MenuItem::action(
                    format!("{} ↗", I18n::t(l, "app", "app.neongammonManual")),
                    OpenNeonGammonManual,
                ),
                MenuItem::separator(),
                MenuItem::action("Versions & Diagnostics", OpenAbout),
            ],
            disabled: false,
        },
    ]
}

/// Installs NeonDojo's native menu bar on the GPUI application.
pub fn install_app_menus(cx: &mut App, state: &MenuState) {
    cx.set_menus(build_app_menus(state));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_menu_bar_contains_all_eight_top_level_menus() {
        let menus = build_app_menus(&MenuState::default());
        let names: Vec<&str> = menus.iter().map(|m| m.name.as_ref()).collect();
        assert_eq!(
            names,
            vec![
                "NeonDojo", "File", "Edit", "View", "Profile", "Analysis", "Window", "Help"
            ]
        );
    }
}
