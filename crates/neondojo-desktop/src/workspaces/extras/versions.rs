//! Extras → Version Info (`VersionInfo.tsx`).
//!
//! Features:
//! - App release/commit from `facade.version()` (sync, works even without DB)
//! - Independent GNUbg (`facade.check_gnubg`) & NeonGammon (`facade.get_open_sage`)
//!   capability checks plus engine catalog inspection
//! - Copy diagnostics to clipboard (`cx.write_to_clipboard`)
//! - GitHub source link via `cx.open_url`

use crate::app_context::DesktopAppContext;
use crate::i18n::{current_locale, format_domain_error};
use crate::theme::{NeonTheme, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::workspaces::extras::spawn_facade_read;
use gpui_kit::{
    ClipboardItem, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Task, TestSupportExt, Window,
    base::StyledExt, div, prelude::FluentBuilder, px,
};
use neondojo::api::{VersionInfo as BackendVersionInfo, version_info};
use neondojo::settings::{EngineInfo, OpenSageInstallation};
use neondojo_i18n::{I18n, Locale};

pub const NEONDOJO_GITHUB_URL: &str = "https://github.com/HendrikReh/neondojo";

/// Extracts a concise numeric version token (e.g. `"1.08.003"` or `"3.2.2"`)
/// from a full engine version details string.
pub fn short_engine_version(engine_id: &str, details: Option<&str>) -> Option<String> {
    let raw = details?.trim();
    if raw.is_empty() {
        return None;
    }
    let stripped = if engine_id == "gnubg" {
        raw.strip_prefix("GNU Backgammon")
            .or_else(|| raw.strip_prefix("GNUbg"))
            .unwrap_or(raw)
            .trim_start()
    } else if engine_id == "bgsage" {
        raw.strip_prefix("NeonGammon").unwrap_or(raw).trim_start()
    } else {
        raw
    };
    let token = stripped
        .split(|c: char| c.is_whitespace() || c == ';' || c == '(')
        .find(|tok| {
            let t = tok
                .strip_prefix('v')
                .or_else(|| tok.strip_prefix('V'))
                .unwrap_or(tok);
            t.chars().next().is_some_and(|c| c.is_ascii_digit()) && t.contains('.')
        })?;
    Some(token.to_string())
}

/// Formats multi-line diagnostics for the system clipboard, matching `VersionInfo.tsx`.
#[allow(clippy::too_many_arguments)]
pub fn format_diagnostics_clipboard(
    locale: Locale,
    app_release: &str,
    app_commit: Option<&str>,
    gnubg_details: Option<&str>,
    gnubg_available: bool,
    sage_details: Option<&str>,
    sage_available: bool,
    fork_url: Option<&str>,
) -> String {
    let unavailable = I18n::t(locale, "app", "versions.unavailable");
    let available = I18n::t(locale, "version-info", "available");
    let mut lines = vec![
        format!(
            "{}: {}",
            I18n::t(locale, "version-info", "appVersion"),
            if app_release.is_empty() {
                unavailable
            } else {
                app_release
            }
        ),
        format!(
            "{}: {}",
            I18n::t(locale, "version-info", "appCommit"),
            app_commit.filter(|c| !c.is_empty()).unwrap_or(unavailable)
        ),
        format!(
            "GNU Backgammon: {}",
            gnubg_details
                .filter(|d| !d.trim().is_empty())
                .unwrap_or(if gnubg_available {
                    available
                } else {
                    unavailable
                })
        ),
        format!(
            "NeonGammon: {}",
            sage_details
                .filter(|d| !d.trim().is_empty())
                .unwrap_or(if sage_available {
                    available
                } else {
                    unavailable
                })
        ),
    ];
    if let Some(url) = fork_url {
        lines.push(format!(
            "{}: {url}",
            I18n::t(locale, "app", "versions.openSageFork")
        ));
    }
    lines.join("\n")
}

#[derive(Debug, Clone)]
enum CheckStatus<T> {
    Idle,
    Checking,
    Ready(T),
    Error(SharedString),
}

/// Complete GPUI Version Information sub-workspace.
pub struct VersionInfoView {
    visible: bool,
    app_version: BackendVersionInfo,
    engines: Option<Vec<EngineInfo>>,
    gnubg_check: CheckStatus<String>,
    sage_install: CheckStatus<OpenSageInstallation>,
    details_open: bool,
    copied_notice: bool,
    _gnubg_task: Option<Task<()>>,
    _sage_task: Option<Task<()>>,
    _engines_task: Option<Task<()>>,
}

impl VersionInfoView {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_version = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
            .map(|f| f.version())
            .unwrap_or_else(version_info);
        Self {
            visible: false,
            app_version,
            engines: None,
            gnubg_check: CheckStatus::Idle,
            sage_install: CheckStatus::Idle,
            details_open: false,
            copied_notice: false,
            _gnubg_task: None,
            _sage_task: None,
            _engines_task: None,
        }
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if visible {
            self.refresh_all(cx);
        }
    }

    fn refresh_all(&mut self, cx: &mut Context<Self>) {
        if let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        {
            self.app_version = facade.version();
        }
        self.load_engines_catalog(cx);
        self.run_gnubg_check(cx);
        self.run_sage_check(cx);
    }

    fn load_engines_catalog(&mut self, cx: &mut Context<Self>) {
        let Some(read_task) =
            spawn_facade_read(cx, move |facade| async move { facade.list_engines().await })
        else {
            return;
        };
        self._engines_task = Some(cx.spawn(async move |this, cx| {
            let Ok(Ok(catalog)) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |v, cx| {
                v.engines = Some(catalog);
                cx.notify();
            });
        }));
    }

    /// Runs an independent GNUbg capability check (`facade.check_gnubg`).
    fn run_gnubg_check(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let fallback_bin = cx
            .try_global::<DesktopAppContext>()
            .map(|ctx| ctx.bootstrap.gnubg_binary.clone())
            .unwrap_or_else(|| "gnubg".into());
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            let (path, threads) = match facade.get_settings().await {
                Ok(s) => (s.gnubg_path, s.gnubg_threads),
                Err(_) => (fallback_bin, None),
            };
            facade.check_gnubg(path, threads).await
        }) else {
            return;
        };
        self.gnubg_check = CheckStatus::Checking;
        cx.notify();

        self._gnubg_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |v, cx| {
                v.gnubg_check = match res {
                    Ok(resp) => CheckStatus::Ready(resp.version),
                    Err(err) => CheckStatus::Error(format_domain_error(locale, &err).into()),
                };
                cx.notify();
            });
        }));
    }

    /// Runs an independent NeonGammon installation status read (`facade.get_open_sage`).
    fn run_sage_check(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let Some(read_task) =
            spawn_facade_read(
                cx,
                move |facade| async move { facade.get_open_sage().await },
            )
        else {
            return;
        };
        self.sage_install = CheckStatus::Checking;
        cx.notify();

        self._sage_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |v, cx| {
                v.sage_install = match res {
                    Ok(install) => CheckStatus::Ready(install),
                    Err(err) => CheckStatus::Error(format_domain_error(locale, &err).into()),
                };
                cx.notify();
            });
        }));
    }

    fn gnubg_info(&self) -> (bool, Option<String>) {
        match &self.gnubg_check {
            CheckStatus::Ready(ver) => (true, Some(ver.clone())),
            _ => {
                let eng = self
                    .engines
                    .as_ref()
                    .and_then(|list| list.iter().find(|e| e.id == "gnubg"));
                match eng {
                    Some(e) if e.available => (true, e.version.clone()),
                    _ => (false, None),
                }
            }
        }
    }

    fn sage_info(&self) -> (bool, Option<String>, Option<(String, String)>) {
        if let CheckStatus::Ready(install) = &self.sage_install
            && let Some(snap) = &install.snapshot
        {
            let details = format!(
                "NeonGammon {} · {} · {}",
                snap.package_version,
                &snap.artifact_fingerprint[..snap.artifact_fingerprint.len().min(12)],
                snap.python
            );
            let fork = snap
                .known_fork()
                .map(|f| (f.repository.to_string(), f.revision.to_string()));
            return (true, Some(details), fork);
        }
        let eng = self
            .engines
            .as_ref()
            .and_then(|list| list.iter().find(|e| e.id == "bgsage"));
        match eng {
            Some(e) if e.available => (
                true,
                e.version.clone(),
                e.fork
                    .as_ref()
                    .map(|f| (f.repository.to_string(), f.revision.to_string())),
            ),
            _ => (false, None, None),
        }
    }

    fn copy_diagnostics(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let (gnubg_avail, gnubg_details) = self.gnubg_info();
        let (sage_avail, sage_details, fork) = self.sage_info();
        let fork_url = fork
            .as_ref()
            .map(|(repo, rev)| format!("https://github.com/{repo}/commit/{rev}"));
        let commit = (!self.app_version.commit.is_empty()).then_some(self.app_version.commit);
        let text = format_diagnostics_clipboard(
            locale,
            self.app_version.release,
            commit,
            gnubg_details.as_deref(),
            gnubg_avail,
            sage_details.as_deref(),
            sage_avail,
            fork_url.as_deref(),
        );
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied_notice = true;
        cx.notify();
    }
}

impl Render for VersionInfoView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let entity = cx.entity().clone();

        let commit_opt = (!self.app_version.commit.is_empty()).then_some(self.app_version.commit);
        let app_label = match commit_opt {
            Some(c) => format!("{} ({})", self.app_version.release, &c[..c.len().min(7)]),
            None => self.app_version.release.to_string(),
        };

        let (gnubg_avail, gnubg_details) = self.gnubg_info();
        let gnubg_short = short_engine_version("gnubg", gnubg_details.as_deref());
        let gnubg_checking = matches!(self.gnubg_check, CheckStatus::Checking);

        let (sage_avail, sage_details, fork) = self.sage_info();
        let sage_short = short_engine_version("bgsage", sage_details.as_deref());
        let sage_checking = matches!(self.sage_install, CheckStatus::Checking);
        let fork_url = fork
            .as_ref()
            .map(|(repo, rev)| format!("https://github.com/{repo}/commit/{rev}"));

        let status_badge = |avail: bool, checking: bool| {
            let (bg, fg, label) = if checking {
                (
                    p.surface_alt,
                    p.muted,
                    I18n::t(locale, "app", "versions.loading"),
                )
            } else if avail {
                (
                    p.success_bg,
                    p.success_text,
                    I18n::t(locale, "version-info", "available"),
                )
            } else {
                (
                    p.surface_alt,
                    p.muted,
                    I18n::t(locale, "app", "versions.unavailable"),
                )
            };
            div()
                .text_meta()
                .font_weight(FontWeight::MEDIUM)
                .px(px(8.0))
                .py(px(2.0))
                .rounded(px(4.0))
                .bg(bg)
                .text_color(fg)
                .child(label)
        };

        let ent_gnubg = entity.clone();
        let ent_sage = entity.clone();
        let ent_details = entity.clone();
        let ent_copy = entity.clone();

        div()
            .id("version-info-view")
            .test_support()
            .v_flex()
            .gap(px(14.0))
            .max_w(px(760.0))
            .child(
                div()
                    .v_flex()
                    .gap(px(10.0))
                    .p(px(16.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    // NeonDojo row
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .py(px(6.0))
                            .border_b_1()
                            .border_color(p.border)
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child("NeonDojo"),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap(px(10.0))
                                    .child(
                                        div()
                                            .mono_font()
                                            .text_body()
                                            .text_color(p.text)
                                            .child(app_label),
                                    )
                                    .child(
                                        ActionButton::new("version-open-github")
                                            .icon(ActionIconName::Link)
                                            .label("GitHub")
                                            .ghost()
                                            .palette(p)
                                            .on_click(|_, _, cx| {
                                                cx.open_url(NEONDOJO_GITHUB_URL);
                                            }),
                                    ),
                            ),
                    )
                    // GNUbg row
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .py(px(6.0))
                            .border_b_1()
                            .border_color(p.border)
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child("GNU Backgammon"),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .when_some(gnubg_short, |this, ver| {
                                        this.child(
                                            div()
                                                .mono_font()
                                                .text_body()
                                                .text_color(p.text)
                                                .child(ver),
                                        )
                                    })
                                    .child(status_badge(gnubg_avail, gnubg_checking))
                                    .child(
                                        ActionButton::new("version-check-gnubg")
                                            .icon(ActionIconName::Refresh)
                                            .tooltip(I18n::t(locale, "app", "versions.retry"))
                                            .ghost()
                                            .dense_nav()
                                            .disabled(gnubg_checking)
                                            .palette(p)
                                            .on_click(move |_, _, cx| {
                                                ent_gnubg.update(cx, |v, cx| v.run_gnubg_check(cx));
                                            }),
                                    ),
                            ),
                    )
                    // NeonGammon row
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .py(px(6.0))
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child("NeonGammon"),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .when_some(sage_short, |this, ver| {
                                        this.child(
                                            div()
                                                .mono_font()
                                                .text_body()
                                                .text_color(p.text)
                                                .child(ver),
                                        )
                                    })
                                    .child(status_badge(sage_avail, sage_checking))
                                    .child(
                                        ActionButton::new("version-check-sage")
                                            .icon(ActionIconName::Refresh)
                                            .tooltip(I18n::t(locale, "app", "versions.retry"))
                                            .ghost()
                                            .dense_nav()
                                            .disabled(sage_checking)
                                            .palette(p)
                                            .on_click(move |_, _, cx| {
                                                ent_sage.update(cx, |v, cx| v.run_sage_check(cx));
                                            }),
                                    ),
                            ),
                    ),
            )
            // Independent check errors
            .when_some(
                match &self.gnubg_check {
                    CheckStatus::Error(err) => Some(format!("GNU Backgammon: {err}")),
                    _ => None,
                },
                |this, msg| {
                    this.child(
                        StatusBanner::new("version-gnubg-error", BannerKind::Warning, msg)
                            .palette(p),
                    )
                },
            )
            .when_some(
                match &self.sage_install {
                    CheckStatus::Error(err) => Some(format!("NeonGammon: {err}")),
                    _ => None,
                },
                |this, msg| {
                    this.child(
                        StatusBanner::new("version-sage-error", BannerKind::Warning, msg)
                            .palette(p),
                    )
                },
            )
            // Technical details
            .child(
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .p(px(14.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        ActionButton::new("version-details-toggle")
                            .icon(if self.details_open {
                                ActionIconName::Collapse
                            } else {
                                ActionIconName::Expand
                            })
                            .label(I18n::t(locale, "version-info", "details"))
                            .ghost()
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_details.update(cx, |v, cx| {
                                    v.details_open = !v.details_open;
                                    cx.notify();
                                });
                            }),
                    )
                    .when(self.details_open, |this| {
                        this.child(
                            div()
                                .v_flex()
                                .gap(px(4.0))
                                .pt(px(6.0))
                                .border_t_1()
                                .border_color(p.border)
                                .text_meta()
                                .text_color(p.muted)
                                .child(div().mono_font().child(format!(
                                    "{}: {}",
                                    I18n::t(locale, "version-info", "appVersion"),
                                    self.app_version.release
                                )))
                                .when_some(commit_opt, |d, c| {
                                    d.child(div().mono_font().child(format!(
                                        "{}: {c}",
                                        I18n::t(locale, "version-info", "appCommit")
                                    )))
                                })
                                .when_some(gnubg_details.clone(), |d, g| {
                                    d.child(div().mono_font().child(format!("GNU Backgammon: {g}")))
                                })
                                .when_some(sage_details.clone(), |d, s| {
                                    d.child(div().mono_font().child(format!("NeonGammon: {s}")))
                                })
                                .when_some(fork_url.clone(), |d, url| {
                                    let label = fork
                                        .as_ref()
                                        .map(|(repo, rev)| {
                                            format!("{repo} {}", &rev[..rev.len().min(10)])
                                        })
                                        .unwrap_or_else(|| url.clone());
                                    d.child(
                                        div()
                                            .h_flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(I18n::t(locale, "app", "versions.openSageFork"))
                                            .child(
                                                div()
                                                    .id("version-open-fork-url")
                                                    .cursor_pointer()
                                                    .text_color(p.accent)
                                                    .on_click(move |_, _, cx| {
                                                        cx.open_url(&url);
                                                    })
                                                    .child(label),
                                            ),
                                    )
                                }),
                        )
                    }),
            )
            // Copy diagnostics strip
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        ActionButton::new("version-copy-btn")
                            .icon(ActionIconName::Copy)
                            .label(I18n::t(locale, "version-info", "copy"))
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_copy.update(cx, |v, cx| v.copy_diagnostics(cx));
                            }),
                    )
                    .when(self.copied_notice, |this| {
                        this.child(div().text_meta().text_color(p.success_text).child(I18n::t(
                            locale,
                            "version-info",
                            "copied",
                        )))
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_short_engine_version_and_formats_diagnostics_clipboard() {
        assert_eq!(
            short_engine_version("gnubg", Some("GNU Backgammon 1.08.003 (macOS)")).as_deref(),
            Some("1.08.003")
        );
        assert_eq!(
            short_engine_version("bgsage", Some("NeonGammon 3.2.2; model abcd")).as_deref(),
            Some("3.2.2")
        );
        let clip = format_diagnostics_clipboard(
            Locale::En,
            "1.4.2",
            Some("abcdef123456"),
            Some("GNU Backgammon 1.08.003"),
            true,
            Some("NeonGammon 3.2.2"),
            true,
            Some("https://github.com/HendrikReh/neongammon/commit/81aec61"),
        );
        assert!(clip.contains("NeonDojo version: 1.4.2"));
        assert!(clip.contains("NeonDojo commit: abcdef123456"));
        assert!(clip.contains("GNU Backgammon: GNU Backgammon 1.08.003"));
        assert!(clip.contains("NeonGammon: NeonGammon 3.2.2"));
        assert!(clip.contains("https://github.com/HendrikReh/neongammon/commit/81aec61"));
    }
}
