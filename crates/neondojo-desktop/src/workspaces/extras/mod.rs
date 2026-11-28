//! Extras workspace: Engine Benchmark, Engine Duels, Database Backup, and Version Info.
//!
//! Mirrors `web/src/workspaces/extras/ExtrasWorkspace.tsx`: keeps all 4 sub-tools
//! mounted so live push subscriptions, active forms, and table state survive
//! tab switches, normalizes `"duels"` / `"matches"` → `"duels"`, persists the
//! active tool in [`LocalUiState::extras_tool`], and forwards child events
//! (`SetBenchmarkBusy`, `OpenMatchInAnalysis`, `TriggerAppReset`) to `AppShell`.

pub mod backup;
pub mod benchmark;
pub mod engine_matches;
pub mod versions;

use crate::app_context::DesktopAppContext;
use crate::events::WorkspaceEvent;
use crate::i18n::current_locale;
use crate::local_state::LocalUiState;
use crate::theme::{NeonTheme, NeonTypographyExt};
use crate::tokio_bridge::{ReadTask, TokioBridge};
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem, TextNavOrientation};
use backup::{DatabaseBackupEvent, DatabaseBackupWorkspace};
use benchmark::{BenchmarkEvent, BenchmarkWorkspace};
use engine_matches::{EngineMatchEvent, EngineMatchWorkspace};
use gpui_kit::{
    App, AppContext, BorrowAppContext, Context, Entity, EventEmitter, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled,
    Subscription, TestSupportExt, Window, base::StyledExt, div, px,
};
use neondojo::facade::BackendFacade;
use neondojo_i18n::I18n;
use std::future::Future;
use versions::VersionInfoView;

/// Sub-tools hosted inside the Extras workspace.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtrasTool {
    #[default]
    Benchmark,
    Duels,
    Backup,
    Versions,
}

impl ExtrasTool {
    pub const ALL: &'static [Self] = &[Self::Benchmark, Self::Duels, Self::Backup, Self::Versions];

    /// Normalizes `"benchmark"`, `"duels"` / `"matches"`, `"backup"`, and `"versions"`.
    /// Unknown values fall back to [`Self::Benchmark`].
    pub fn parse_lossy(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "duels" | "matches" => Self::Duels,
            "backup" => Self::Backup,
            "versions" => Self::Versions,
            _ => Self::Benchmark,
        }
    }

    /// Canonical persisted identifier matching [`LocalUiState::extras_tool`].
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Benchmark => "benchmark",
            Self::Duels => "duels",
            Self::Backup => "backup",
            Self::Versions => "versions",
        }
    }

    /// Tab element ID matching Playwright/desktop test selectors.
    pub const fn tab_element_id(self) -> &'static str {
        match self {
            Self::Benchmark => "extras-tab-benchmark",
            Self::Duels => "extras-tab-duels",
            Self::Backup => "extras-tab-backup",
            Self::Versions => "extras-tab-versions",
        }
    }
}

/// Runs an abortable read operation against [`BackendFacade`] on Tokio's
/// blocking thread pool to sidestep cross-crate SQLx HRTB `Send` limitations.
pub(crate) fn spawn_facade_read<F, Fut, R>(cx: &App, op: F) -> Option<ReadTask<R>>
where
    F: FnOnce(BackendFacade) -> Fut + Send + 'static,
    Fut: Future<Output = R>,
    R: Send + 'static,
{
    let facade = cx.try_global::<DesktopAppContext>()?.facade()?;
    if !cx.has_global::<TokioBridge>() {
        return None;
    }
    Some(TokioBridge::spawn_read(cx, async move {
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || handle.block_on(op(facade)))
            .await
            .expect("facade read task panicked")
    }))
}

/// Runs a non-abortable write mutation against [`BackendFacade`] on Tokio's
/// blocking thread pool (dropping observation never cancels the backend write).
pub(crate) fn spawn_facade_write<F, Fut, R>(
    cx: &App,
    op: F,
) -> Option<gpui_kit::Task<Result<R, tokio::task::JoinError>>>
where
    F: FnOnce(BackendFacade) -> Fut + Send + 'static,
    Fut: Future<Output = R>,
    R: Send + 'static,
{
    let facade = cx.try_global::<DesktopAppContext>()?.facade()?;
    if !cx.has_global::<TokioBridge>() {
        return None;
    }
    Some(TokioBridge::spawn_write(cx, async move {
        let handle = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || handle.block_on(op(facade)))
            .await
            .expect("facade write task panicked")
    }))
}

/// Short human-readable engine label (`"bgsage"` → `"NeonGammon"`, `"gnubg"` → `"GNUbg"`).
pub(crate) fn engine_display_name(id: &str) -> &str {
    match id {
        "gnubg" => "GNUbg",
        "bgsage" => "NeonGammon",
        other => other,
    }
}

/// Top-level GPUI Extras workspace hosting all 4 sub-tools.
pub struct ExtrasWorkspace {
    visible: bool,
    tool: ExtrasTool,
    benchmark: Entity<BenchmarkWorkspace>,
    duels: Entity<EngineMatchWorkspace>,
    backup: Entity<DatabaseBackupWorkspace>,
    versions: Entity<VersionInfoView>,
    _subscriptions: Vec<Subscription>,
}

impl ExtrasWorkspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let initial_tool = cx
            .try_global::<LocalUiState>()
            .map(|s| ExtrasTool::parse_lossy(&s.extras_tool))
            .unwrap_or_default();

        let benchmark = cx.new(|cx| BenchmarkWorkspace::new(window, cx));
        let duels = cx.new(|cx| EngineMatchWorkspace::new(window, cx));
        let backup = cx.new(|cx| DatabaseBackupWorkspace::new(window, cx));
        let versions = cx.new(|cx| VersionInfoView::new(window, cx));

        let mut subscriptions = Vec::new();
        subscriptions.push(
            cx.subscribe(
                &benchmark,
                |_this, _, event: &BenchmarkEvent, cx| match event {
                    BenchmarkEvent::BusyChanged(active) => {
                        cx.emit(WorkspaceEvent::SetBenchmarkBusy(*active));
                    }
                },
            ),
        );
        subscriptions.push(cx.subscribe(
            &duels,
            |_this, _, event: &EngineMatchEvent, cx| match event {
                EngineMatchEvent::OpenMatchInAnalysis(match_id) => {
                    cx.emit(WorkspaceEvent::OpenMatchInAnalysis {
                        match_id: *match_id,
                        game_no: None,
                        position_id: None,
                        decision_no: None,
                    });
                }
            },
        ));
        subscriptions.push(
            cx.subscribe(
                &backup,
                |_this, _, event: &DatabaseBackupEvent, cx| match event {
                    DatabaseBackupEvent::Restored => {
                        cx.emit(WorkspaceEvent::TriggerAppReset {
                            reason: SharedString::from("backup_restore"),
                        });
                    }
                },
            ),
        );

        let workspace = Self {
            visible: true,
            tool: initial_tool,
            benchmark,
            duels,
            backup,
            versions,
            _subscriptions: subscriptions,
        };
        workspace.sync_child_visibility(cx);
        workspace
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.visible == visible {
            return;
        }
        self.visible = visible;
        self.sync_child_visibility(cx);
        cx.notify();
    }

    /// Selects an Extras sub-tool (`"benchmark"`, `"duels"` / `"matches"`,
    /// `"backup"`, `"versions"`) and persists the canonical ID into
    /// [`LocalUiState::extras_tool`].
    pub fn select_tool(&mut self, tool: &str, cx: &mut Context<Self>) {
        let next = ExtrasTool::parse_lossy(tool);
        let changed = self.tool != next;
        self.tool = next;
        if cx.has_global::<LocalUiState>() {
            cx.update_global::<LocalUiState, _>(|state, _| {
                state.extras_tool = next.as_str().to_string();
            });
        }
        if changed {
            self.sync_child_visibility(cx);
            cx.notify();
        }
    }

    pub fn active_tool(&self) -> ExtrasTool {
        self.tool
    }

    fn sync_child_visibility(&self, cx: &mut Context<Self>) {
        let visible = self.visible;
        let tool = self.tool;
        self.benchmark.update(cx, |child, cx| {
            child.set_visible(visible && tool == ExtrasTool::Benchmark, cx);
        });
        self.duels.update(cx, |child, cx| {
            child.set_visible(visible && tool == ExtrasTool::Duels, cx);
        });
        self.backup.update(cx, |child, cx| {
            child.set_visible(visible && tool == ExtrasTool::Backup, cx);
        });
        self.versions.update(cx, |child, cx| {
            child.set_visible(visible && tool == ExtrasTool::Versions, cx);
        });
    }
}

impl EventEmitter<WorkspaceEvent> for ExtrasWorkspace {}

impl Render for ExtrasWorkspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let entity = cx.entity().clone();

        let nav_items = vec![
            TextNavItem::new("benchmark", I18n::t(locale, "benchmark", "benchmark"))
                .element_id(ExtrasTool::Benchmark.tab_element_id()),
            TextNavItem::new("duels", I18n::t(locale, "engine-match", "title"))
                .element_id(ExtrasTool::Duels.tab_element_id()),
            TextNavItem::new("backup", I18n::t(locale, "database-backup", "title"))
                .element_id(ExtrasTool::Backup.tab_element_id()),
            TextNavItem::new("versions", I18n::t(locale, "version-info", "tab"))
                .element_id(ExtrasTool::Versions.tab_element_id()),
        ];

        let nav = TextNavBar::new("extras-sub-nav", nav_items)
            .orientation(TextNavOrientation::Horizontal)
            .density(TextNavDensity::Inline)
            .active(Some(self.tool.as_str()))
            .palette(p)
            .on_select(move |id, _window, cx| {
                let target = id.to_string();
                entity.update(cx, |this, cx| {
                    this.select_tool(&target, cx);
                });
            });

        let active_panel = match self.tool {
            ExtrasTool::Benchmark => self.benchmark.clone().into_any_element(),
            ExtrasTool::Duels => self.duels.clone().into_any_element(),
            ExtrasTool::Backup => self.backup.clone().into_any_element(),
            ExtrasTool::Versions => self.versions.clone().into_any_element(),
        };

        div()
            .id("extras-workspace")
            .test_support()
            .v_flex()
            .size_full()
            .min_h(px(0.0))
            .bg(p.bg)
            .text_color(p.text)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .px(px(20.0))
                    .pt(px(12.0))
                    .pb(px(6.0))
                    .border_b_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(nav)
                    .child(div().text_meta().text_color(p.muted).child(I18n::t(
                        locale,
                        "benchmark",
                        "navigation",
                    ))),
            )
            .child(
                div()
                    .id("extras-workspace-body")
                    .v_flex()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(px(20.0))
                    .child(active_panel),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configure_app;
    use gpui_kit::{WindowOptions, test::TestWindowExt};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn select_tool_normalizes_duels_matches_and_unknown_values() {
        assert_eq!(ExtrasTool::parse_lossy("benchmark"), ExtrasTool::Benchmark);
        assert_eq!(ExtrasTool::parse_lossy("duels"), ExtrasTool::Duels);
        assert_eq!(ExtrasTool::parse_lossy("matches"), ExtrasTool::Duels);
        assert_eq!(ExtrasTool::parse_lossy("MATCHES"), ExtrasTool::Duels);
        assert_eq!(ExtrasTool::parse_lossy("backup"), ExtrasTool::Backup);
        assert_eq!(ExtrasTool::parse_lossy("versions"), ExtrasTool::Versions);
        assert_eq!(ExtrasTool::parse_lossy("unknown"), ExtrasTool::Benchmark);
        assert_eq!(ExtrasTool::Duels.as_str(), "duels");
    }

    #[gpui_kit::test]
    async fn extras_workspace_renders_switches_tools_persists_state_and_forwards_events(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let busy_seen = Arc::new(AtomicBool::new(false));
        let reset_reason = Arc::new(std::sync::Mutex::new(None::<String>));
        let opened_match = Arc::new(std::sync::Mutex::new(None::<uuid::Uuid>));

        let busy_flag = Arc::clone(&busy_seen);
        let reset_slot = Arc::clone(&reset_reason);
        let opened_slot = Arc::clone(&opened_match);

        let (window, workspace) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, move |window, cx| {
                    let ws = cx.new(|cx| ExtrasWorkspace::new(window, cx));
                    cx.subscribe(&ws, move |_, event: &WorkspaceEvent, _| match event {
                        WorkspaceEvent::SetBenchmarkBusy(active) => {
                            busy_flag.store(*active, Ordering::SeqCst);
                        }
                        WorkspaceEvent::TriggerAppReset { reason } => {
                            *reset_slot.lock().unwrap() = Some(reason.to_string());
                        }
                        WorkspaceEvent::OpenMatchInAnalysis { match_id, .. } => {
                            *opened_slot.lock().unwrap() = Some(*match_id);
                        }
                        _ => {}
                    })
                    .detach();
                    ws
                })
            })
            .expect("open extras window");

        let sample_match_id = uuid::Uuid::new_v4();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("extras-workspace").visible());

            // Switch across all 4 tabs, including `"matches"` -> `"duels"` normalization.
            for (input, expected, expected_panel_id) in [
                ("matches", ExtrasTool::Duels, "engine-match-workspace"),
                ("backup", ExtrasTool::Backup, "database-backup-workspace"),
                ("versions", ExtrasTool::Versions, "version-info-view"),
                ("benchmark", ExtrasTool::Benchmark, "benchmark-workspace"),
                ("duels", ExtrasTool::Duels, "engine-match-workspace"),
            ] {
                workspace.update(cx, |ws, cx| ws.select_tool(input, cx));
                assert_eq!(workspace.read(cx).active_tool(), expected);
                assert_eq!(cx.global::<LocalUiState>().extras_tool, expected.as_str());
                window.render_frame(cx);
                assert!(
                    window.find(expected_panel_id).visible(),
                    "expected {expected_panel_id} to be visible for tool {input}"
                );
            }

            // Clicking sub-tab in TextNavBar switches tools too.
            window.click("extras-tab-backup", cx);
            window.render_frame(cx);
            assert_eq!(workspace.read(cx).active_tool(), ExtrasTool::Backup);

            // Verify child events bubble cleanly to WorkspaceEvent.
            let bench = workspace.read(cx).benchmark.clone();
            bench.update(cx, |_, cx| cx.emit(BenchmarkEvent::BusyChanged(true)));
            let duels = workspace.read(cx).duels.clone();
            duels.update(cx, |_, cx| {
                cx.emit(EngineMatchEvent::OpenMatchInAnalysis(sample_match_id));
            });
            let backup = workspace.read(cx).backup.clone();
            backup.update(cx, |ws, cx| ws.notify_restored(cx));
        })
        .expect("update extras window");

        cx.run_until_parked();

        assert!(busy_seen.load(Ordering::SeqCst));
        assert_eq!(*opened_match.lock().unwrap(), Some(sample_match_id));
        assert_eq!(
            reset_reason.lock().unwrap().as_deref(),
            Some("backup_restore")
        );
    }
}
