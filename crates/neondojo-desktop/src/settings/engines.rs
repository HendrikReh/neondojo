//! Settings → Engines section (`EngineSettingsSection.tsx` & `OpenSageInstallation.tsx`).
//!
//! Covers host-local GNUbg executable path & check (`check_gnubg`), NeonGammon
//! Python path & immediate verify/save (`verify_open_sage`), 4 thread limits,
//! and parallel analysis jobs (`1..=8`).

use crate::i18n::current_locale;
use crate::settings::draft_guard::SettingsDraft;
use crate::theme::{NeonPalette, NeonTypographyExt};
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use gpui_kit::{
    App, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString, Styled,
    Window,
    base::StyledExt,
    component::input::{Input, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::settings::OpenSageInstallation;
use neondojo_i18n::{I18n, Param};
use neondojo_rules::options::{engine_default_threads, engine_min_threads, parallel_analysis_jobs};
use std::rc::Rc;

/// Which of the 4 host thread counts or parallel job slots is being adjusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostConcurrencyField {
    GnubgAnalysis,
    GnubgPlay,
    SageAnalysis,
    SagePlay,
    ParallelJobs,
}

pub type EngineActionHandler = Rc<dyn Fn(&mut Window, &mut App) + 'static>;
pub type ConcurrencyPickHandler =
    Rc<dyn Fn(HostConcurrencyField, Option<i32>, &mut Window, &mut App) + 'static>;

/// Callbacks for host-local engine checks & concurrency pickers.
#[derive(Clone)]
pub struct EngineSectionCallbacks {
    pub on_check_gnubg: EngineActionHandler,
    pub on_verify_sage: EngineActionHandler,
    pub on_concurrency: ConcurrencyPickHandler,
}

/// Renders Settings → Engines.
#[allow(clippy::too_many_arguments)]
pub fn render_engines_section(
    draft: &SettingsDraft,
    gnubg_path_input: &Entity<InputState>,
    sage_path_input: &Entity<InputState>,
    sage_installation: Option<&OpenSageInstallation>,
    checking_gnubg: bool,
    verifying_sage: bool,
    gnubg_check_error: Option<&str>,
    sage_error: Option<&str>,
    disabled: bool,
    palette: NeonPalette,
    cb: EngineSectionCallbacks,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let max_threads = std::thread::available_parallelism()
        .map(|n| i32::try_from(n.get()).unwrap_or(8))
        .unwrap_or(8)
        .clamp(2, 16);

    let (min_jobs, max_jobs, default_jobs) = parallel_analysis_jobs();

    let sage_status_summary = match sage_installation.and_then(|i| i.snapshot.as_ref()) {
        Some(snap) => {
            let mut lines = vec![
                I18n::format(
                    locale,
                    "settings",
                    "sageLastVersion",
                    &[("version", Param::from(snap.package_version.as_str()))],
                ),
                I18n::format(
                    locale,
                    "settings",
                    "sageModelIdentity",
                    &[(
                        "fingerprint",
                        Param::from(
                            &snap.artifact_fingerprint[..snap.artifact_fingerprint.len().min(12)],
                        ),
                    )],
                ),
            ];
            let depths = if snap.one_ply_verified {
                "1-ply, 2-ply, 3-ply, 4-ply"
            } else {
                "2-ply, 3-ply, 4-ply"
            };
            lines.push(I18n::format(
                locale,
                "settings",
                "sageSupportedDepths",
                &[("depths", Param::from(depths))],
            ));
            lines.join(" · ")
        }
        None => I18n::t(locale, "settings", "sageUnverified").to_string(),
    };

    div()
        .id("settings-section-engines")
        .v_flex()
        .gap(px(16.0))
        .child(div().text_meta().text_color(palette.muted).child(I18n::t(
            locale,
            "settings",
            "localEnginesHelp",
        )))
        // GNUbg installation
        .child(
            div()
                .v_flex()
                .gap(px(8.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(palette.border)
                .bg(palette.surface_alt)
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_panel().text_color(palette.text).child(I18n::t(
                            locale,
                            "settings",
                            "installation",
                        )))
                        .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                            locale,
                            "settings",
                            "engineSaveWithApply",
                        ))),
                )
                .child(
                    div()
                        .h_flex()
                        .gap(px(8.0))
                        .child(
                            div().flex_1().child(
                                Input::new(gnubg_path_input).id("settings-gnubg-path-input"),
                            ),
                        )
                        .child(
                            ActionButton::new("settings-check-gnubg")
                                .icon(ActionIconName::Refresh)
                                .label(if checking_gnubg {
                                    I18n::t(locale, "settings", "checking")
                                } else {
                                    I18n::t(locale, "settings", "check")
                                })
                                .disabled(disabled || checking_gnubg)
                                .palette(palette)
                                .on_click({
                                    let on_check = cb.on_check_gnubg.clone();
                                    move |_, window, cx| on_check(window, cx)
                                }),
                        ),
                )
                .when_some(draft.gnubg_checked_version.clone(), |this, ver| {
                    this.child(
                        StatusBanner::new(
                            "gnubg-check-ok",
                            BannerKind::Success,
                            I18n::format(
                                locale,
                                "settings",
                                "checkSuccess",
                                &[("version", Param::from(ver.as_str()))],
                            ),
                        )
                        .palette(palette),
                    )
                })
                .when_some(gnubg_check_error, |this, err| {
                    this.child(
                        StatusBanner::new("gnubg-check-err", BannerKind::Error, err.to_string())
                            .palette(palette),
                    )
                })
                .when(
                    draft.gnubg_edited && draft.gnubg_checked_version.is_none(),
                    |this| {
                        this.child(
                            StatusBanner::new(
                                "gnubg-check-needed",
                                BannerKind::Warning,
                                I18n::t(locale, "settings", "checkNeeded"),
                            )
                            .palette(palette),
                        )
                    },
                )
                .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                    locale,
                    "settings",
                    "checkHelp",
                ))),
        )
        // NeonGammon (bgsage) installation — verifies and saves immediately
        .child(
            div()
                .v_flex()
                .gap(px(8.0))
                .p(px(12.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(palette.border)
                .bg(palette.surface_alt)
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_panel()
                                .text_color(palette.text)
                                .child(I18n::t(locale, "settings", "planned")),
                        )
                        .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                            locale,
                            "settings",
                            "engineSaveImmediate",
                        ))),
                )
                .child(
                    div()
                        .h_flex()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(sage_path_input).id("settings-sage-path-input")),
                        )
                        .child(
                            ActionButton::new("settings-verify-sage")
                                .icon(ActionIconName::Confirm)
                                .label(if verifying_sage {
                                    I18n::t(locale, "settings", "sageVerifying")
                                } else {
                                    I18n::t(locale, "settings", "sageVerify")
                                })
                                .disabled(disabled || verifying_sage)
                                .palette(palette)
                                .on_click({
                                    let on_verify = cb.on_verify_sage.clone();
                                    move |_, window, cx| on_verify(window, cx)
                                }),
                        ),
                )
                .child(
                    div()
                        .text_meta()
                        .text_color(palette.muted)
                        .child(sage_status_summary),
                )
                .when_some(sage_error, |this, err| {
                    this.child(
                        StatusBanner::new("sage-verify-err", BannerKind::Error, err.to_string())
                            .palette(palette),
                    )
                })
                .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                    locale,
                    "settings",
                    "verifySageTarget",
                ))),
        )
        // Threads & Parallel jobs
        .child(
            div()
                .v_flex()
                .gap(px(10.0))
                .pt(px(8.0))
                .border_t_1()
                .border_color(palette.border)
                .child(div().text_panel().text_color(palette.text).child(I18n::t(
                    locale,
                    "settings",
                    "engineThreadsHeading",
                )))
                .child(render_thread_selector(
                    "gnubg-analysis-threads",
                    I18n::t(locale, "settings", "gnubgAnalysisThreads"),
                    draft.draft_host.gnubg_threads,
                    engine_min_threads("gnubg"),
                    max_threads,
                    engine_default_threads("gnubg", "analysis"),
                    HostConcurrencyField::GnubgAnalysis,
                    disabled,
                    palette,
                    cb.on_concurrency.clone(),
                    cx,
                ))
                .child(render_thread_selector(
                    "gnubg-play-threads",
                    I18n::t(locale, "settings", "gnubgPlayThreads"),
                    draft.draft_host.gnubg_play_threads,
                    engine_min_threads("gnubg"),
                    max_threads,
                    engine_default_threads("gnubg", "play"),
                    HostConcurrencyField::GnubgPlay,
                    disabled,
                    palette,
                    cb.on_concurrency.clone(),
                    cx,
                ))
                .child(render_thread_selector(
                    "sage-analysis-threads",
                    I18n::t(locale, "settings", "sageAnalysisThreads"),
                    draft.draft_host.open_sage_threads,
                    engine_min_threads("bgsage"),
                    max_threads,
                    engine_default_threads("bgsage", "analysis"),
                    HostConcurrencyField::SageAnalysis,
                    disabled,
                    palette,
                    cb.on_concurrency.clone(),
                    cx,
                ))
                .child(render_thread_selector(
                    "sage-play-threads",
                    I18n::t(locale, "settings", "sagePlayThreads"),
                    draft.draft_host.open_sage_play_threads,
                    engine_min_threads("bgsage"),
                    max_threads,
                    engine_default_threads("bgsage", "play"),
                    HostConcurrencyField::SagePlay,
                    disabled,
                    palette,
                    cb.on_concurrency.clone(),
                    cx,
                ))
                .child(render_thread_selector(
                    "parallel-analysis-jobs",
                    I18n::t(locale, "settings", "parallelAnalysisJobs"),
                    draft.draft_host.parallel_analysis_jobs,
                    min_jobs,
                    max_jobs,
                    default_jobs,
                    HostConcurrencyField::ParallelJobs,
                    disabled,
                    palette,
                    cb.on_concurrency,
                    cx,
                ))
                .child(div().text_meta().text_color(palette.muted).child(I18n::t(
                    locale,
                    "settings",
                    "parallelAnalysisJobsHelp",
                ))),
        )
}

#[allow(clippy::too_many_arguments)]
fn render_thread_selector(
    id_prefix: &'static str,
    label: &'static str,
    current: Option<i32>,
    min_val: i32,
    max_val: i32,
    default_val: i32,
    field: HostConcurrencyField,
    disabled: bool,
    palette: NeonPalette,
    on_pick: ConcurrencyPickHandler,
    cx: &App,
) -> impl IntoElement {
    let locale = current_locale(cx);
    let default_label = I18n::format(
        locale,
        "settings",
        "defaultThreads",
        &[("count", Param::from(default_val))],
    );

    let mut row = div().flex().flex_row().flex_wrap().gap(px(4.0)).child({
        let on_pick = on_pick.clone();
        ActionButton::new(SharedString::from(format!("{id_prefix}-default")))
            .label(default_label)
            .active(current.is_none())
            .disabled(disabled)
            .palette(palette)
            .on_click(move |_, window, cx| on_pick(field, None, window, cx))
    });

    // Offer compact power-of-2 / sequential choices within [min_val, max_val]
    let candidates: Vec<i32> = if max_val <= 8 {
        (min_val..=max_val).collect()
    } else {
        [1, 2, 4, 6, 8, 12, 16]
            .into_iter()
            .filter(|&v| v >= min_val && v <= max_val)
            .collect()
    };

    for val in candidates {
        let on_pick = on_pick.clone();
        row = row.child(
            ActionButton::new(SharedString::from(format!("{id_prefix}-{val}")))
                .label(val.to_string())
                .active(current == Some(val))
                .disabled(disabled)
                .palette(palette)
                .on_click(move |_, window, cx| on_pick(field, Some(val), window, cx)),
        );
    }

    div()
        .v_flex()
        .gap(px(4.0))
        .child(
            div()
                .text_label()
                .font_weight(FontWeight::MEDIUM)
                .text_color(palette.text)
                .child(label),
        )
        .child(row)
}
