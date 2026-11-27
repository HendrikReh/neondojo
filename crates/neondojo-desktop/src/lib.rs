//! NeonDojo native macOS desktop shell built on GPUI (`gpui-kit`).

pub mod anki_webview;
pub mod app_context;
pub mod assets;
pub mod events;
pub mod i18n;
pub mod keybindings;
pub mod local_state;
pub mod review;
pub mod settings;
pub mod shell;
pub mod theme;
pub mod tokio_bridge;
pub mod ui;
pub mod workspaces;

use app_context::{DesktopAppContext, resolve_bootstrap_config};
use assets::NeonAssets;
use gpui_kit::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use i18n::init_locale;
use keybindings::{MenuState, Quit, install_app_menus, register_keybindings};
use local_state::LocalUiState;
use neondojo_i18n::Locale;
use shell::AppShell;
use theme::{NeonTheme, ResolvedMode};
use tokio_bridge::TokioBridge;

/// Initializes NeonDojo's global state, live `neondojo-i18n` locale, Finder-safe
/// bootstrap context, theme projection, keyboard actions, and native macOS menu bar.
pub fn configure_app(state: LocalUiState, system_dark: bool, cx: &mut App) {
    let locale = Locale::parse(&state.locale);
    let palette_id = state.appearance.theme;
    let mode = state.appearance.mode;
    let bootstrap = resolve_bootstrap_config(&state);

    init_locale(locale, cx);
    cx.set_global(DesktopAppContext::new(bootstrap));
    cx.set_global(state);
    NeonTheme::init(palette_id, mode, system_dark, cx);
    register_keybindings(cx);
    cx.on_action(|_: &Quit, cx| {
        cx.quit();
    });
    install_app_menus(
        cx,
        &MenuState {
            locale,
            palette: palette_id,
            mode,
            ..MenuState::default()
        },
    );
}

/// Launches the NeonDojo native GPUI desktop application.
pub fn run() {
    let state_path = LocalUiState::default_path();
    let initial_state = LocalUiState::load_or_default(&state_path);

    gpui_kit::application()
        .with_assets(NeonAssets)
        .run(move |cx: &mut App| {
            gpui_kit::init(cx);
            TokioBridge::init(cx).expect("initialize Tokio runtime for NeonDojo");
            configure_app(initial_state.clone(), false, cx);

            let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..WindowOptions::default()
            };

            let persist_path = state_path.clone();
            gpui_kit::open_window(options, cx, move |window, cx| {
                let system_dark =
                    ResolvedMode::from_window_appearance(window.appearance()).is_dark();
                NeonTheme::set(
                    initial_state.appearance.theme,
                    initial_state.appearance.mode,
                    system_dark,
                    cx,
                );
                cx.new(|cx| {
                    let mut shell = AppShell::new(Some(persist_path), window, cx);
                    shell.boot_backend_async(window, cx);
                    shell
                })
            })
            .expect("open NeonDojo desktop window");
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{current_locale, set_locale};
    use crate::review::cube_details::{
        CubeDecisionDetailsView, CubeEquityUnit, CubeEvaluationData,
    };
    use crate::review::move_explanation::{MoveExplanationView, explain_checker_plays};
    use crate::review::move_history_row::{AssessmentSource, MoveAssessmentBadge, MoveHistoryRow};
    use crate::review::preview_controls::PreviewBoardControls;
    use crate::settings::draft_guard::SettingsCategory;
    use crate::shell::{AppNotice, AppResetEvent, AppView, WorkspaceId};
    use crate::theme::{NeonPalette, NeonPaletteId, ThemeMode};
    use crate::ui::banners::BannerKind;
    use gpui_kit::{
        IntoElement, ParentElement, Render, Styled,
        base::StyledExt,
        component::{WindowExt, theme::Theme},
        div, px,
        test::TestWindowExt,
    };
    use neondojo::model::MoveSeverity;
    use neondojo_board::BoardViewMode;
    use neondojo_i18n::AppError;
    use neondojo_rules::backgammon::initial_board;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    };
    use std::time::Duration;
    use tempfile::tempdir;

    #[gpui_kit::test]
    async fn all_six_palettes_apply_to_gpui_theme_and_meet_wcag_contrast(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        for &palette_id in NeonPaletteId::ALL {
            for &resolved_mode in ResolvedMode::ALL {
                let theme_mode = match resolved_mode {
                    ResolvedMode::Light => ThemeMode::Light,
                    ResolvedMode::Dark => ThemeMode::Dark,
                };
                cx.update(|cx| {
                    NeonTheme::set(palette_id, theme_mode, resolved_mode.is_dark(), cx);
                    let expected = NeonPalette::resolve(palette_id, resolved_mode);
                    let active = NeonTheme::current_palette(cx);
                    assert_eq!(active, expected);

                    let gpui_theme = Theme::global(cx);
                    assert_eq!(gpui_theme.colors.background, expected.bg);
                    assert_eq!(gpui_theme.colors.foreground, expected.text);
                    assert_eq!(gpui_theme.colors.primary, expected.accent);
                    assert_eq!(gpui_theme.colors.input, expected.control_border);

                    for (surface_name, surface_hex) in [
                        ("bg", expected.bg_hex),
                        ("surface", expected.surface_hex),
                        ("surface_alt", expected.surface_alt_hex),
                    ] {
                        let ratio = expected.control_border_hex.contrast_ratio(surface_hex);
                        assert!(
                            ratio >= 3.0,
                            "{:?}/{:?} control_border vs {surface_name} contrast {ratio:.2} < 3.0",
                            palette_id,
                            resolved_mode
                        );
                    }
                });
            }
        }
    }

    #[gpui_kit::test]
    async fn workspace_navigation_clicks_shortcuts_lazy_mount_and_app_reset(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let reset_gen_seen = Arc::new(AtomicU64::new(0));
        let reset_seen_clone = Arc::clone(&reset_gen_seen);

        let (window, shell) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    let entity = cx.new(|cx| AppShell::new(None, window, cx));
                    cx.subscribe(&entity, move |_, event: &AppResetEvent, _| {
                        reset_seen_clone.store(event.generation, Ordering::SeqCst);
                    })
                    .detach();
                    entity
                })
            })
            .expect("open test window");

        let analysis_entity_id = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(shell.read(cx).active_view(), AppView::Home);
                assert_eq!(shell.read(cx).mounted_workspace_count(), 0);
                assert!(window.find("home-screen").visible());

                window.click("workspace-tab-analysis", cx);
                window.render_frame(cx);
                assert_eq!(
                    shell.read(cx).active_view(),
                    AppView::Workspace(WorkspaceId::Analysis)
                );
                assert_eq!(shell.read(cx).mounted_workspace_count(), 1);
                let analysis_slot = shell
                    .read(cx)
                    .workspace_slot(WorkspaceId::Analysis)
                    .expect("analysis slot lazily created");
                assert_eq!(analysis_slot.read(cx).visit_count(), 1);
                let analysis_id = analysis_slot.entity_id();

                for (shortcut, expected) in [
                    ("cmd-1", WorkspaceId::Play),
                    ("cmd-3", WorkspaceId::Study),
                    ("cmd-4", WorkspaceId::Positions),
                    ("cmd-5", WorkspaceId::Library),
                    ("cmd-6", WorkspaceId::Players),
                    ("cmd-7", WorkspaceId::Extras),
                ] {
                    window.press(shortcut, cx);
                    window.render_frame(cx);
                    assert_eq!(shell.read(cx).active_view(), AppView::Workspace(expected));
                }

                window.press("cmd-2", cx);
                window.render_frame(cx);
                let retained = shell
                    .read(cx)
                    .workspace_slot(WorkspaceId::Analysis)
                    .expect("analysis slot retained");
                assert_eq!(retained.entity_id(), analysis_id);
                assert_eq!(retained.read(cx).visit_count(), 2);

                window.press("cmd-0", cx);
                window.render_frame(cx);
                assert_eq!(shell.read(cx).active_view(), AppView::Home);
                assert_eq!(shell.read(cx).mounted_workspace_count(), 7);

                analysis_id
            })
            .expect("update window for workspace navigation");

        cx.update_window(window, |_, window, cx| {
            shell.update(cx, |s, cx| {
                s.trigger_app_reset("backup_restore", cx);
            });
            window.render_frame(cx);
            assert_eq!(shell.read(cx).reset_generation(), 1);
            assert_eq!(shell.read(cx).mounted_workspace_count(), 0);
            assert_eq!(shell.read(cx).active_view(), AppView::Home);

            window.press("cmd-2", cx);
            window.render_frame(cx);
            let remounted = shell
                .read(cx)
                .workspace_slot(WorkspaceId::Analysis)
                .expect("fresh analysis slot mounted");
            assert_ne!(remounted.entity_id(), analysis_entity_id);
            assert_eq!(remounted.read(cx).mount_generation(), 1);
            assert_eq!(remounted.read(cx).visit_count(), 1);
        })
        .expect("update window for reset");

        assert_eq!(reset_gen_seen.load(Ordering::SeqCst), 1);
    }

    #[gpui_kit::test]
    async fn modal_busy_lock_blocks_escape_until_cleared(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let (window, shell) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                    cx.new(|cx| AppShell::new(None, window, cx))
                })
            })
            .expect("open test window");

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(!window.has_active_dialog(cx));

            window.click("header-settings-button", cx);
            window.render_frame(cx);
            assert!(window.has_active_dialog(cx));
            assert!(window.find("settings-modal-body").visible());

            let busy_lock = shell.read(cx).settings_modal_busy_lock();
            busy_lock.set_busy(true);
            window.render_frame(cx);

            window.press("escape", cx);
            window.render_frame(cx);
            assert!(
                window.has_active_dialog(cx),
                "Escape must be blocked while ModalBusyLock is active"
            );

            busy_lock.set_busy(false);
            window.render_frame(cx);

            window.press("escape", cx);
            window.render_frame(cx);
            assert!(
                !window.has_active_dialog(cx),
                "Escape must dismiss modal once busy lock clears"
            );
        })
        .expect("update test window for modal busy lock");
    }

    #[gpui_kit::test]
    async fn settings_dialog_supports_all_categories_scoped_preview_draft_guard_and_live_i18n(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let dir = tempdir().expect("temp dir");
        let state_path = dir.path().join("ui-state.json");

        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let persist_for_shell = state_path.clone();
        let (window, shell) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, move |window, cx| {
                    cx.new(|cx| AppShell::new(Some(persist_for_shell), window, cx))
                })
            })
            .expect("open test window");

        cx.update_window(window, |_, window, cx| {
            // Seed a re-translatable AppError notice to prove live EN -> DE updates.
            shell.update(cx, |s, cx| {
                s.push_notice(
                    AppNotice {
                        id: "i18n-err".into(),
                        kind: BannerKind::Error,
                        title: None,
                        message: AppError::with_params(
                            "app",
                            "error.request",
                            [("status", 503.into())],
                        )
                        .into(),
                        dismissible: true,
                    },
                    cx,
                );
            });
            window.render_frame(cx);
            assert_eq!(current_locale(cx), Locale::En);

            // Open Settings via Cmd+,
            window.press("cmd-,", cx);
            window.render_frame(cx);
            assert!(window.has_active_dialog(cx));

            // Select German in General (must NOT mutate live locale until Apply)
            window.click("settings-locale-de", cx);
            window.render_frame(cx);
            assert_eq!(current_locale(cx), Locale::En);

            // Switch to Appearance: choose Club + Dark -> scoped preview stays isolated until Apply
            window.click("settings-cat-appearance", cx);
            window.render_frame(cx);
            assert!(window.find("settings-ui-appearance-preview").visible());
            window.click("settings-palette-club", cx);
            window.click("settings-mode-dark", cx);
            window.render_frame(cx);
            assert_eq!(NeonTheme::current_palette(cx).id, NeonPaletteId::Graphite);

            // Visit Board (renders BoardView + BoardGalleryTile) and Analysis
            window.click("settings-cat-board", cx);
            window.render_frame(cx);
            assert!(window.find("board-gallery-forest").visible());

            window.click("settings-cat-analysis", cx);
            window.render_frame(cx);

            // Attempting to close while dirty opens stacked SettingsDraftGuardDialog
            window.click("settings-modal-done", cx);
            window.render_frame(cx);
            assert!(window.find("settings-draft-guard-body").visible());

            // Click Apply & Continue inside the draft guard -> applies live DE + Club Dark and closes!
            window.click("draft-guard-apply", cx);
            window.render_frame(cx);

            assert!(!window.has_active_dialog(cx));
            assert_eq!(current_locale(cx), Locale::De);
            let active = NeonTheme::current_palette(cx);
            assert_eq!(active.id, NeonPaletteId::Club);
            assert_eq!(active.mode, ResolvedMode::Dark);

            // Verify live re-translation of AppError notice & workspace labels
            assert_eq!(
                shell.read(cx).notices()[0].message.resolve(cx).as_ref(),
                "Anfrage fehlgeschlagen (HTTP 503)."
            );
            assert_eq!(WorkspaceId::Play.localized_label(Locale::De), "Spielen");

            // Switch back to English via helper to confirm live sync
            set_locale(Locale::En, cx);
            assert_eq!(current_locale(cx), Locale::En);

            // Open Settings directly at Profiles category
            shell.update(cx, |s, cx| {
                s.open_settings_modal_at(SettingsCategory::Profiles, window, cx);
            });
            window.render_frame(cx);
            assert_eq!(
                shell
                    .read(cx)
                    .active_settings_view()
                    .unwrap()
                    .read(cx)
                    .category(),
                SettingsCategory::Profiles
            );
        })
        .expect("update window for full settings lifecycle");

        let reloaded = LocalUiState::load_or_default(&state_path);
        assert_eq!(reloaded.locale, "de");
        assert_eq!(reloaded.appearance.theme, NeonPaletteId::Club);
        assert_eq!(reloaded.appearance.mode, ThemeMode::Dark);
    }

    struct ReviewShowcase;
    impl Render for ReviewShowcase {
        fn render(
            &mut self,
            _window: &mut gpui_kit::Window,
            _cx: &mut gpui_kit::Context<Self>,
        ) -> impl IntoElement {
            let explanation =
                explain_checker_plays(initial_board(), 0, [3, 1], "8/5 6/5", "13/10 24/23")
                    .expect("explain opening 3-1");

            div()
                .v_flex()
                .gap(px(12.0))
                .p(px(16.0))
                .child(
                    PreviewBoardControls::new("review-preview-controls", BoardViewMode::Arrows)
                        .preview_summary("White", "8/5 6/5")
                        .can_reset(true),
                )
                .child(
                    MoveHistoryRow::new("review-row-1", "1", 0, "Alice", "8/5 6/5")
                        .dice(vec![3, 1])
                        .board_i32(&initial_board())
                        .assessment(Some(MoveAssessmentBadge {
                            equity_loss: 0.085,
                            severity: MoveSeverity::Blunder,
                            source: AssessmentSource::Engine,
                            engine: "NeonGammon".into(),
                            depth: Some("3-ply".into()),
                        }))
                        .selected(true),
                )
                .child(
                    CubeDecisionDetailsView::new(
                        "review-cube-details",
                        0,
                        false,
                        ["Alice".into(), "Bob".into()],
                    )
                    .match_id("MAAAAAAAAAAA")
                    .evaluation(Some(CubeEvaluationData {
                        no_double: 1.12,
                        double_take: 1.24,
                        double_pass: 1.0,
                        unit: CubeEquityUnit::NormalizedCubeful,
                        probabilities: Some([0.78, 0.36, 0.04, 0.22, 0.03, 0.002]),
                    })),
                )
                .child(
                    MoveExplanationView::from_explanation(
                        "review-move-explanation",
                        "8/5 6/5",
                        "13/10 24/23",
                        &explanation,
                        None,
                        None,
                    )
                    .show_glossary(true),
                )
        }
    }

    #[gpui_kit::test]
    async fn shared_review_foundation_renders_cleanly_in_headless_gpui(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            configure_app(LocalUiState::default(), false, cx);
        });

        let (window, _view) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                    cx.new(|_| ReviewShowcase)
                })
            })
            .expect("open review showcase window");

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("review-preview-controls").visible());
            assert!(window.find("review-row-1").visible());
            assert!(window.find("review-cube-details").visible());
            assert!(window.find("cube-too-good-callout").visible());
            assert!(window.find("cube-chances-table").visible());
            assert!(window.find("cube-take-point").visible());
            assert!(window.find("review-move-explanation").visible());
        })
        .expect("render review showcase");
    }

    #[gpui_kit::test]
    async fn tokio_bridge_aborts_dropped_reads_but_preserves_dropped_writes(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            TokioBridge::init(cx).expect("init tokio bridge");
        });

        let read_completed = Arc::new(AtomicBool::new(false));
        let read_flag = Arc::clone(&read_completed);
        let read_task = cx.update(|cx| {
            TokioBridge::spawn_read(cx, async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                read_flag.store(true, Ordering::SeqCst);
            })
        });
        drop(read_task);

        let write_completed = Arc::new(AtomicBool::new(false));
        let write_flag = Arc::clone(&write_completed);
        let write_task = cx.update(|cx| {
            TokioBridge::spawn_write(cx, async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                write_flag.store(true, Ordering::SeqCst);
            })
        });
        drop(write_task);

        std::thread::sleep(Duration::from_millis(260));
        assert!(
            !read_completed.load(Ordering::SeqCst),
            "dropped read task must be aborted"
        );
        assert!(
            write_completed.load(Ordering::SeqCst),
            "dropped write task must complete"
        );
    }
}
