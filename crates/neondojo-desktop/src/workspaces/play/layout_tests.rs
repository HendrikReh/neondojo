//! Exercise painted board geometry, pointer dispatch, and dialog clipping in a desktop viewport.
use super::*;
use crate::keybindings::register_keybindings;
use crate::theme::{NeonPaletteId, ThemeMode};
use gpui_kit::{
    AnyWindowHandle, Bounds, TestAppContext, WindowBounds, WindowOptions, size, test::TestWindowExt,
};
use neondojo_i18n::Locale;
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

struct Frame(Entity<PlayWorkspace>);

impl Render for Frame {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // Leave room for the application header and footer around our isolated workspace.
        div()
            .v_flex()
            .size_full()
            .pt(px(76.))
            .pb(px(42.))
            .child(self.0.clone())
    }
}

fn open_play(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<PlayWorkspace>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        NeonTheme::init(NeonPaletteId::Graphite, ThemeMode::Light, false, cx);
        register_keybindings(cx);
        cx.set_global(LocalUiState::default());
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                Default::default(),
                size(px(1440.), px(900.)),
            ))),
            ..Default::default()
        };
        let (window, frame) = gpui_kit::open_window(opts, cx, |window, cx| {
            let play = cx.new(|cx| PlayWorkspace::new_for_test(window, cx));
            cx.new(|_| Frame(play))
        })
        .unwrap();
        (window, frame.read(cx).0.clone())
    })
}

#[gpui_kit::test]
async fn new_match_shows_all_settings_and_keeps_actions_visible_when_resized(
    cx: &mut TestAppContext,
) {
    let (window, play) = open_play(cx);
    for locale in Locale::LANGUAGES {
        for engine in ["gnubg", "bgsage"] {
            for mode in [PlayMode::Training, PlayMode::Tutor] {
                cx.update_window(window, |_, window, cx| {
                    crate::i18n::set_locale(locale, cx);
                    play.update(cx, |ws, cx| {
                        ws.setup_choices.select_engine(engine);
                        ws.setup_choices.mode = mode;
                        cx.notify();
                    });
                    window.render_frame(cx);
                    let content = window.find("play-setup-form").bounds();
                    let body = window.find("play-setup-dialog-body").bounds();
                    let start = window.find("play-setup-start").bounds();
                    let workspace = window.find("play-workspace").bounds();
                    assert!(
                        content.bottom() <= body.bottom() + px(1.),
                        "{locale:?} {engine} {mode:?}: body {body:?}, content {content:?}"
                    );
                    assert!(start.top() >= body.bottom() && start.bottom() <= workspace.bottom());
                })
                .unwrap();
            }
        }
    }
    let started = Rc::new(Cell::new(false));
    let observed = started.clone();
    cx.update(|cx| {
        cx.observe_keystrokes(move |event, _, _| {
            if event
                .action
                .as_ref()
                .is_some_and(|action| action.partial_eq(&StartPlayMatch))
            {
                observed.set(true);
            }
        })
        .detach()
    });
    cx.simulate_window_resize(window, size(px(680.), px(520.)));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let content = window.find("play-setup-form").bounds();
        let body = window.find("play-setup-dialog-body").bounds();
        let card = window.find("play-setup-dialog").bounds();
        let workspace = window.find("play-workspace").bounds();
        let start = window.find("play-setup-start").bounds();
        assert!(card.top() >= workspace.top() && card.bottom() <= workspace.bottom());
        assert!(content.size.height > body.size.height && body.size.height > px(80.));
        assert!(start.top() >= body.bottom() && start.bottom() <= card.bottom());
        window.scroll(
            "play-setup-dialog-body",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.0), px(-300.0))),
            cx,
        );
        assert!(window.find("play-setup-form").bounds().top() < content.top());
        assert_eq!(window.find("play-setup-start").bounds(), start);
        play.update(cx, |ws, cx| ws.open_new_match_setup(window, cx));
        window.press("cmd-enter", cx);
    })
    .unwrap();
    assert!(
        started.get(),
        "the focused compact dialog should handle the Start shortcut"
    );
}

#[gpui_kit::test]
async fn painted_sources_complete_a_checker_turn_across_a_resize(cx: &mut TestAppContext) {
    let (window, play) = open_play(cx);
    let board = cx.update(|cx| play.read(cx).board_stage.read(cx).board_view().clone());
    cx.update_window(window, |_, window, cx| {
        window.click("play-setup-dialog-close", cx);
        window.render_frame(cx);
        let idle = board.read(cx).last_hitboxes().board_bounds;
        assert!(
            idle.size.width > px(600.) && idle.size.height > px(400.),
            "{idle:?}"
        );
    })
    .unwrap();

    let confirmed = Rc::new(Cell::new(None));
    cx.update(|cx| {
        let observed = confirmed.clone();
        cx.subscribe(&play.read(cx).board_stage.clone(), move |_, event, _| {
            if let PlayStageEvent::Confirm(index) = event {
                observed.set(Some(*index));
            }
        })
        .detach();
        let session = super::tests::make_session(PlayPhase::Move, PlayMode::Competition);
        play.read(cx).session_entity().clone().update(cx, |se, cx| {
            se.set_session_for_test(session, false, cx);
        });
    });

    let animation = cx.update(|cx| play.read(cx).animation_entity().clone());
    for (source, width, height) in [(8, 1440., 900.), (6, 1024., 720.)] {
        cx.simulate_window_resize(window, size(px(width), px(height)));
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let hits = board.read(cx).last_hitboxes();
            let target = hits
                .point_targets
                .iter()
                .find(|(n, _)| *n == source)
                .unwrap()
                .1;
            assert!(
                hits.board_bounds.size.width > px(420.) && target.size.width > px(18.),
                "{width}×{height}: {:?}, {target:?}",
                hits.board_bounds
            );
            assert_eq!(hits.hit_test_point(target.center()), Some(source));
            let origin = window.find("play-live-board").bounds().origin;
            window.click_at("play-live-board", target.center() - origin, cx);
        })
        .unwrap();
        cx.update_window(window, |_, window, cx| {
            let before = animation.read(cx).tick_count();
            window.simulate_next_frame(cx);
            assert!(
                animation.read(cx).tick_count() > before,
                "a source click must schedule its own frame"
            );
            // Finish this flight deterministically, without sleeping the UI thread.
            animation.update(cx, |a, cx| {
                a.sample_at(Instant::now() + Duration::from_secs(1), cx)
            });
        })
        .unwrap();
    }
    cx.update_window(window, |_, window, cx| {
        assert_eq!(
            play.read(cx).turn_entity().read(cx).step_pairs(),
            [(8, 5), (6, 5)]
        );
        window.click("play-action-confirm", cx);
    })
    .unwrap();
    assert_eq!(confirmed.get(), Some(0));
}
