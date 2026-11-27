//! NeonDojo native macOS desktop shell built on GPUI (`gpui-kit`).

pub mod tokio_bridge;

use gpui_kit::{
    App, AppContext, Context, IntoElement, KeyBinding, Menu, MenuItem, ParentElement, Render,
    Styled, Task, Window, WindowOptions, actions,
    base::StyledExt,
    component::button::{Button, ButtonVariants},
    div, px,
};
use neondojo::api::{VersionInfo, version_info};
use neondojo_rules::backgammon;
use tokio_bridge::TokioBridge;

actions!(neondojo, [Quit]);

/// Minimal Phase 0 GPUI shell proving:
/// - `gpui-kit 0.11` rendering and action wiring
/// - Synchronous shared rules (`neondojo-rules`) on the UI thread
/// - In-process backend metadata (`neondojo`) and `TokioBridge` async tasks
pub struct DesktopShell {
    version: VersionInfo,
    opening_31_paths: usize,
    clicks: usize,
    async_pings: usize,
    ping_task: Option<Task<()>>,
}

impl DesktopShell {
    pub fn new() -> Self {
        let board = backgammon::initial_board();
        let opening_31_paths = backgammon::checker_paths(&board, 0, [3, 1])
            .map(|paths| paths.len())
            .unwrap_or(0);
        Self {
            version: version_info(),
            opening_31_paths,
            clicks: 0,
            async_pings: 0,
            ping_task: None,
        }
    }

    pub fn clicks(&self) -> usize {
        self.clicks
    }

    pub fn async_pings(&self) -> usize {
        self.async_pings
    }

    pub fn opening_31_paths(&self) -> usize {
        self.opening_31_paths
    }

    fn trigger_tokio_ping(&mut self, cx: &mut Context<Self>) {
        let read_task = TokioBridge::spawn_read(cx, async move { 1usize });
        self.ping_task = Some(cx.spawn(async move |this, cx| {
            if let Ok(delta) = read_task.await {
                let _ = this.update(cx, |shell, cx| {
                    shell.async_pings += delta;
                    cx.notify();
                });
            }
        }));
    }
}

impl Default for DesktopShell {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for DesktopShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_3()
            .p(px(24.0))
            .size_full()
            .items_center()
            .justify_center()
            .child(format!(
                "NeonDojo {} (GPUI Phase 0 Shell)",
                self.version.release
            ))
            .child(format!(
                "Opening 3-1 legal checker paths (neondojo-rules): {}",
                self.opening_31_paths
            ))
            .child(format!(
                "Clicks: {} | Tokio pings: {}",
                self.clicks, self.async_pings
            ))
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .child(
                        Button::new("phase0-increment")
                            .primary()
                            .label("Increment")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.clicks += 1;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("phase0-tokio-ping")
                            .label("Ping Tokio")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.trigger_tokio_ping(cx);
                            })),
                    ),
            )
    }
}

/// Registers standard NeonDojo macOS menus and global quit action.
pub fn configure_app(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    cx.on_action(|_: &Quit, cx| {
        cx.quit();
    });
    cx.set_menus(vec![Menu {
        name: "NeonDojo".into(),
        items: vec![MenuItem::action("Quit NeonDojo", Quit)],
        disabled: false,
    }]);
}

/// Launches the Phase 0 GPUI desktop application.
pub fn run() {
    gpui_kit::application().run(|cx: &mut App| {
        gpui_kit::init(cx);
        TokioBridge::init(cx).expect("initialize Tokio runtime for NeonDojo");
        configure_app(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| DesktopShell::new())
        })
        .expect("open NeonDojo desktop window");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::test::TestWindowExt;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::Duration;

    #[gpui_kit::test]
    async fn shell_renders_rules_and_handles_click_and_tokio_ping(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            TokioBridge::init(cx).expect("init tokio bridge");
            configure_app(cx);
        });

        let (window, shell) = cx
            .update(|cx| {
                gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                    cx.new(|_| DesktopShell::new())
                })
            })
            .expect("open test window");

        cx.update_window(window, |_, window, cx| {
            let state = shell.read(cx);
            assert!(state.opening_31_paths() > 0);
            assert_eq!(state.clicks(), 0);
            assert_eq!(state.async_pings(), 0);

            window.render_frame(cx);
            assert!(window.find("phase0-increment").visible());
            window.click("phase0-increment", cx);
            assert_eq!(shell.read(cx).clicks(), 1);

            window.click("phase0-tokio-ping", cx);
        })
        .expect("update test window");

        for _ in 0..40 {
            cx.run_until_parked();
            let pings = cx.read_entity(&shell, |s, _| s.async_pings());
            if pings == 1 {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("expected Tokio ping to update DesktopShell");
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
        // Dropping the read Task cancels the Tokio future immediately.
        drop(read_task);

        let write_completed = Arc::new(AtomicBool::new(false));
        let write_flag = Arc::clone(&write_completed);
        let write_task = cx.update(|cx| {
            TokioBridge::spawn_write(cx, async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                write_flag.store(true, Ordering::SeqCst);
            })
        });
        // Dropping the write Task detaches observation without aborting the write.
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
