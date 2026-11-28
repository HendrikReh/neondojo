//! Extras → Engine Duels (`EngineMatchWorkspace.tsx`, `EngineSeriesSetup.tsx`,
//! `EngineSeriesResults.tsx`).
//!
//! Features:
//! - Master-detail series list with active-first sorting and cursor pagination
//! - Point match (1–25) vs Cubeless Money blocks (1–1,000), Paired vs Random dice,
//!   optional UUID seed, and Repeat-series prefill
//! - Live push updates via `facade.subscribe_engine_match_updates()`
//! - Progress bar, 2-segment win-rate bar, 95% Student-t CI calculation,
//!   gammon/backgammon shares, CPU & wall ms/move
//! - Native macOS Save dialog (`cx.prompt_for_new_path`) for SGF export
//! - Open in Analysis emitting [`EngineMatchEvent::OpenMatchInAnalysis`]

use crate::app_context::DesktopAppContext;
use crate::i18n::{current_locale, format_domain_error};
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::tokio_bridge::TokioBridge;
use crate::ui::action_button::ActionButton;
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::modal::{ModalBusyLock, ModalConfig, ModalSize, open_modal_dialog};
use crate::workspaces::extras::{engine_display_name, spawn_facade_read, spawn_facade_write};
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task,
    TestSupportExt, Window,
    base::StyledExt,
    component::WindowExt,
    component::input::{Input, InputEvent, InputState},
    div,
    prelude::FluentBuilder,
    px,
};
use neondojo::engine_matches::{
    Competitor, CreateSeries, DiceMode, DuelFormat, Series, SeriesConfig, SeriesDetails,
    SeriesPage, SideCostSummary,
};
use neondojo::settings::EngineInfo;
use neondojo_i18n::{
    I18n, Locale, format_benchmark_duration, format_datetime_short_local, format_int,
    format_percent, format_ply, format_signed_decimal,
};
use neondojo_rules::options::engine_depths;
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

pub const MAX_MATCH_LENGTH: i32 = 25;
pub const MAX_BLOCK_GAMES: i32 = 1000;
pub const MAX_SERIES_COUNT: i32 = 1000;
pub const MIN_UNITS_FOR_CI: usize = 20;

/// Events emitted by [`EngineMatchWorkspace`] to [`super::ExtrasWorkspace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineMatchEvent {
    OpenMatchInAnalysis(Uuid),
}

/// 0.975 quantile of Student's t distribution with `df` degrees of freedom
/// (Cornish–Fisher expansion around the normal quantile, Abramowitz & Stegun 26.7.5).
pub fn student_t_975(df: usize) -> f64 {
    const Z: f64 = 1.959_963_984_540_054;
    let n = df.max(1) as f64;
    let z2 = Z * Z;
    let g1 = (z2 + 1.0) * Z / 4.0;
    let g2 = ((5.0 * z2 + 16.0) * z2 + 3.0) * Z / 96.0;
    let g3 = (((3.0 * z2 + 19.0) * z2 + 17.0) * z2 - 15.0) * Z / 384.0;
    let g4 = ((((79.0 * z2 + 776.0) * z2 + 1482.0) * z2 - 1920.0) * z2 - 945.0) * Z / 92_160.0;
    Z + g1 / n + g2 / (n * n) + g3 / (n * n * n) + g4 / (n * n * n * n)
}

/// Two-sided 95% Student-t confidence interval for the sample mean of independent
/// observations, clamped to `[min, max]`. Returns `None` for fewer than `min_samples`
/// observations (or `< 2`).
pub fn compute_student_t_ci_95(
    values: &[f64],
    min: f64,
    max: f64,
    min_samples: usize,
) -> Option<[f64; 2]> {
    let n = values.len();
    if n < min_samples.max(2) {
        return None;
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
    let radius = student_t_975(n - 1) * (variance / n as f64).sqrt();
    Some([(mean - radius).max(min), (mean + radius).min(max)])
}

/// Editable setup form state for starting or repeating an engine duel series.
#[derive(Debug, Clone)]
pub struct SeriesDraft {
    pub a: Competitor,
    pub b: Competitor,
    pub format: DuelFormat,
    pub mode: DiceMode,
    pub match_length: String,
    pub block_games: String,
    pub count: String,
    pub seed: String,
}

impl Default for SeriesDraft {
    fn default() -> Self {
        Self {
            a: Competitor {
                engine_id: "gnubg".into(),
                plies: 2,
            },
            b: Competitor {
                engine_id: "gnubg".into(),
                plies: 1,
            },
            format: DuelFormat::Match,
            mode: DiceMode::Paired,
            match_length: "5".into(),
            block_games: "100".into(),
            count: "1".into(),
            seed: String::new(),
        }
    }
}

impl SeriesDraft {
    /// Prefills a new setup draft from an existing series configuration ("Repeat series …").
    pub fn from_config(config: &SeriesConfig) -> Self {
        let is_money = config.format == DuelFormat::MoneyCubeless;
        Self {
            a: config.a.clone(),
            b: config.b.clone(),
            format: config.format,
            mode: config.mode,
            match_length: if is_money {
                "5".into()
            } else {
                config.match_length.to_string()
            },
            block_games: config.block_games.unwrap_or(100).to_string(),
            count: config.count.to_string(),
            seed: config.seed.to_string(),
        }
    }

    pub fn parsed_seed(&self) -> Result<Option<Uuid>, &'static str> {
        let trimmed = self.seed.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Uuid::parse_str(trimmed)
            .map(Some)
            .map_err(|_| "Seed must be a valid UUID")
    }

    pub fn to_create_request(&self) -> Result<CreateSeries, &'static str> {
        let is_money = self.format == DuelFormat::MoneyCubeless;
        let count = parse_bounded_i32(&self.count, 1, MAX_SERIES_COUNT)
            .ok_or("Series count must be between 1 and 1,000")?;
        let (match_length, block_games) = if is_money {
            let games = parse_bounded_i32(&self.block_games, 1, MAX_BLOCK_GAMES)
                .ok_or("Games per block must be between 1 and 1,000")?;
            (0, Some(games))
        } else {
            let len = parse_bounded_i32(&self.match_length, 1, MAX_MATCH_LENGTH)
                .ok_or("Match length must be between 1 and 25")?;
            (len, None)
        };
        let seed = self.parsed_seed()?;
        Ok(CreateSeries {
            a: self.a.clone(),
            b: self.b.clone(),
            format: self.format,
            match_length,
            block_games,
            count,
            mode: self.mode,
            seed,
        })
    }

    pub fn total_money_games(&self) -> Option<i64> {
        if self.format != DuelFormat::MoneyCubeless {
            return None;
        }
        let games = i64::from(parse_bounded_i32(&self.block_games, 1, MAX_BLOCK_GAMES)?);
        let count = i64::from(parse_bounded_i32(&self.count, 1, MAX_SERIES_COUNT)?);
        let mult = if self.mode == DiceMode::Paired { 2 } else { 1 };
        Some(games * count * mult)
    }
}

pub fn parse_bounded_i32(raw: &str, min: i32, max: i32) -> Option<i32> {
    let val = raw.trim().parse::<i32>().ok()?;
    (min..=max).contains(&val).then_some(val)
}

/// Cloneable snapshot of an engine's duel availability and supported plies.
#[derive(Debug, Clone)]
pub struct DuelEngineOption {
    pub id: String,
    pub name: String,
    pub available: bool,
    pub plies: Vec<i32>,
}

impl From<&EngineInfo> for DuelEngineOption {
    fn from(info: &EngineInfo) -> Self {
        Self {
            id: info.id.clone(),
            name: info.name.clone(),
            available: info.available,
            plies: info.plies.clone(),
        }
    }
}

pub fn duel_depths_for_engine(engine_id: &str, engines: &[DuelEngineOption]) -> Vec<i32> {
    engine_depths(engine_id, "duel")
        .into_iter()
        .filter(|&depth| {
            engine_id != "bgsage"
                || depth != 1
                || engines
                    .iter()
                    .find(|e| e.id == engine_id)
                    .is_some_and(|e| e.plies.contains(&1))
        })
        .collect()
}

fn is_series_active(status: &str) -> bool {
    status == "queued" || status == "running"
}

fn series_status_label(locale: Locale, status: &str) -> String {
    match status {
        "queued" | "running" | "completed" | "failed" | "cancelled" => {
            I18n::t(locale, "engine-match", status).to_string()
        }
        other => other.to_string(),
    }
}

fn series_unit_key(money: bool, paired: bool) -> &'static str {
    match (money, paired) {
        (true, true) => "blockPairs",
        (true, false) => "blocks",
        (false, true) => "pairs",
        (false, false) => "matches",
    }
}

fn series_progress_text(locale: Locale, run: &Series) -> String {
    let unit = if run.configuration.format == DuelFormat::MoneyCubeless {
        "blocks"
    } else {
        "matches"
    };
    format!(
        "{} / {} {}",
        format_int(locale, i64::from(run.completed_matches)),
        format_int(locale, i64::from(run.total_matches)),
        I18n::t(locale, "engine-match", unit)
    )
}

fn competitor_label(locale: Locale, side: &Competitor) -> String {
    format!(
        "{} {}",
        engine_display_name(&side.engine_id),
        format_ply(locale, side.plies as u32)
    )
}

/// Modal view entity for creating / repeating an engine duel series.
struct EngineSeriesSetupDialog {
    draft: SeriesDraft,
    engines: Vec<DuelEngineOption>,
    advanced_open: bool,
    length_input: Entity<InputState>,
    block_games_input: Entity<InputState>,
    count_input: Entity<InputState>,
    seed_input: Entity<InputState>,
    busy: ModalBusyLock,
    error: Option<SharedString>,
    parent: Entity<EngineMatchWorkspace>,
    _subscriptions: Vec<Subscription>,
}

impl EngineSeriesSetupDialog {
    fn new(
        draft: SeriesDraft,
        engines: Vec<DuelEngineOption>,
        advanced_open: bool,
        busy: ModalBusyLock,
        parent: Entity<EngineMatchWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let length_input =
            cx.new(|cx| InputState::new(window, cx).default_value(draft.match_length.clone()));
        let block_games_input =
            cx.new(|cx| InputState::new(window, cx).default_value(draft.block_games.clone()));
        let count_input =
            cx.new(|cx| InputState::new(window, cx).default_value(draft.count.clone()));
        let seed_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(draft.seed.clone())
                .placeholder("xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx")
        });

        let mut subs = Vec::new();
        subs.push(cx.subscribe_in(
            &length_input,
            window,
            |this, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.draft.match_length = state.read(cx).value().to_string();
                    this.error = None;
                    cx.notify();
                }
            },
        ));
        subs.push(cx.subscribe_in(
            &block_games_input,
            window,
            |this, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.draft.block_games = state.read(cx).value().to_string();
                    this.error = None;
                    cx.notify();
                }
            },
        ));
        subs.push(cx.subscribe_in(
            &count_input,
            window,
            |this, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.draft.count = state.read(cx).value().to_string();
                    this.error = None;
                    cx.notify();
                }
            },
        ));
        subs.push(cx.subscribe_in(
            &seed_input,
            window,
            |this, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.draft.seed = state.read(cx).value().to_string();
                    this.error = None;
                    cx.notify();
                }
            },
        ));

        Self {
            draft,
            engines,
            advanced_open,
            length_input,
            block_games_input,
            count_input,
            seed_input,
            busy,
            error: None,
            parent,
            _subscriptions: subs,
        }
    }

    fn engine_available(&self, id: &str) -> bool {
        self.engines.iter().any(|e| e.id == id && e.available)
    }

    fn side_selectable(&self, side: &Competitor) -> bool {
        self.engine_available(&side.engine_id)
            && duel_depths_for_engine(&side.engine_id, &self.engines).contains(&side.plies)
    }

    fn can_submit(&self) -> bool {
        !self.busy.is_busy()
            && self.engine_available("gnubg")
            && self.side_selectable(&self.draft.a)
            && self.side_selectable(&self.draft.b)
            && self.draft.to_create_request().is_ok()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy.is_busy() {
            return;
        }
        let req = match self.draft.to_create_request() {
            Ok(req) => req,
            Err(msg) => {
                self.error = Some(msg.into());
                cx.notify();
                return;
            }
        };
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade.start_engine_match(req).await
        }) else {
            return;
        };
        self.busy.set_busy(true);
        self.error = None;
        cx.notify();

        let parent = self.parent.clone();
        let draft_copy = self.draft.clone();
        cx.spawn_in(window, async move |this, cx| {
            let res = write_task.await;
            let _ = this.update_in(cx, |dlg, window, cx| {
                dlg.busy.set_busy(false);
                match res {
                    Ok(Ok(series)) => {
                        parent.update(cx, |ws, cx| {
                            ws.draft = draft_copy;
                            ws.select_series(series.id, cx);
                            ws.reload_all(cx);
                        });
                        window.close_dialog(cx);
                    }
                    Ok(Err(err)) => {
                        dlg.error = Some(format_domain_error(locale, &err).into());
                        cx.notify();
                    }
                    Err(err) => {
                        dlg.error = Some(err.to_string().into());
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn render_competitor(
        &self,
        is_a: bool,
        p: NeonPalette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let side = if is_a { &self.draft.a } else { &self.draft.b };
        let title_key = if is_a { "engineA" } else { "engineB" };
        let tag = if is_a { "a" } else { "b" };
        let ent = cx.entity().clone();
        let depths = duel_depths_for_engine(&side.engine_id, &self.engines);

        let mut eng_bar = div().h_flex().gap(px(6.0)).flex_wrap();
        for eng in &self.engines {
            let id_owned = eng.id.clone();
            let active = side.engine_id == eng.id;
            let ent_c = ent.clone();
            eng_bar = eng_bar.child(
                ActionButton::new(SharedString::from(format!("duel-{tag}-eng-{}", eng.id)))
                    .label(eng.name.clone())
                    .active(active)
                    .disabled(!eng.available || self.busy.is_busy())
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        let next_id = id_owned.clone();
                        ent_c.update(cx, |dlg, cx| {
                            let target = if is_a {
                                &mut dlg.draft.a
                            } else {
                                &mut dlg.draft.b
                            };
                            target.engine_id = next_id;
                            target.plies = 2;
                            cx.notify();
                        });
                    }),
            );
        }

        let mut depth_bar = div().h_flex().gap(px(6.0)).flex_wrap();
        for depth in depths {
            let active = side.plies == depth;
            let ent_d = ent.clone();
            depth_bar = depth_bar.child(
                ActionButton::new(SharedString::from(format!("duel-{tag}-ply-{depth}")))
                    .label(format_ply(locale, depth as u32))
                    .active(active)
                    .disabled(self.busy.is_busy())
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent_d.update(cx, |dlg, cx| {
                            let target = if is_a {
                                &mut dlg.draft.a
                            } else {
                                &mut dlg.draft.b
                            };
                            target.plies = depth;
                            cx.notify();
                        });
                    }),
            );
        }

        div()
            .v_flex()
            .gap(px(8.0))
            .flex_1()
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface_alt)
            .child(
                div()
                    .text_label()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text)
                    .child(I18n::t(locale, "engine-match", title_key)),
            )
            .child(eng_bar)
            .child(div().text_meta().text_color(p.muted).child(I18n::t(
                locale,
                "engine-match",
                "strength",
            )))
            .child(depth_bar)
    }
}

impl Render for EngineSeriesSetupDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let is_money = self.draft.format == DuelFormat::MoneyCubeless;
        let is_paired = self.draft.mode == DiceMode::Paired;
        let ent = cx.entity().clone();

        let format_bar = div()
            .h_flex()
            .gap(px(6.0))
            .child({
                let e = ent.clone();
                ActionButton::new("duel-format-match")
                    .label(I18n::t(locale, "engine-match", "formatMatch"))
                    .active(!is_money)
                    .disabled(self.busy.is_busy())
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        e.update(cx, |dlg, cx| {
                            dlg.draft.format = DuelFormat::Match;
                            cx.notify();
                        });
                    })
            })
            .child({
                let e = ent.clone();
                ActionButton::new("duel-format-money")
                    .label(I18n::t(locale, "engine-match", "formatMoney"))
                    .active(is_money)
                    .disabled(self.busy.is_busy())
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        e.update(cx, |dlg, cx| {
                            dlg.draft.format = DuelFormat::MoneyCubeless;
                            cx.notify();
                        });
                    })
            });

        let dice_bar = div()
            .h_flex()
            .gap(px(6.0))
            .child({
                let e = ent.clone();
                ActionButton::new("duel-dice-paired")
                    .label(I18n::t(locale, "engine-match", "paired"))
                    .active(is_paired)
                    .disabled(self.busy.is_busy())
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        e.update(cx, |dlg, cx| {
                            dlg.draft.mode = DiceMode::Paired;
                            cx.notify();
                        });
                    })
            })
            .child({
                let e = ent.clone();
                ActionButton::new("duel-dice-random")
                    .label(I18n::t(locale, "engine-match", "random"))
                    .active(!is_paired)
                    .disabled(self.busy.is_busy())
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        e.update(cx, |dlg, cx| {
                            dlg.draft.mode = DiceMode::Random;
                            cx.notify();
                        });
                    })
            });

        let adv_ent = ent.clone();

        div()
            .v_flex()
            .gap(px(12.0))
            .child(
                div()
                    .h_flex()
                    .gap(px(10.0))
                    .child(self.render_competitor(true, p, locale, cx))
                    .child(self.render_competitor(false, p, locale, cx)),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .child(div().text_label().text_color(p.muted).child(I18n::t(
                        locale,
                        "engine-match",
                        "format",
                    )))
                    .child(format_bar),
            )
            .child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .child(div().text_label().text_color(p.muted).child(I18n::t(
                        locale,
                        "engine-match",
                        "dice",
                    )))
                    .child(dice_bar),
            )
            .child(
                div()
                    .h_flex()
                    .gap(px(12.0))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .flex_1()
                            .child(div().text_label().text_color(p.muted).child(I18n::t(
                                locale,
                                "engine-match",
                                if is_money { "blockGames" } else { "length" },
                            )))
                            .child(Input::new(if is_money {
                                &self.block_games_input
                            } else {
                                &self.length_input
                            })),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .flex_1()
                            .child(div().text_label().text_color(p.muted).child(I18n::t(
                                locale,
                                "engine-match",
                                series_unit_key(is_money, is_paired),
                            )))
                            .child(Input::new(&self.count_input)),
                    ),
            )
            .when(is_money, |this| {
                let mut hint = I18n::t(locale, "engine-match", "formatMoneyHint").to_string();
                if let Some(total) = self.draft.total_money_games() {
                    hint.push_str(&format!(
                        " {}: {}",
                        I18n::t(locale, "engine-match", "totalGames"),
                        format_int(locale, total)
                    ));
                }
                this.child(div().text_meta().text_color(p.muted).child(hint))
            })
            .child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .child(
                        ActionButton::new("duel-advanced-toggle")
                            .icon(if self.advanced_open {
                                ActionIconName::Collapse
                            } else {
                                ActionIconName::Expand
                            })
                            .label(I18n::t(locale, "engine-match", "advanced"))
                            .ghost()
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                adv_ent.update(cx, |dlg, cx| {
                                    dlg.advanced_open = !dlg.advanced_open;
                                    cx.notify();
                                });
                            }),
                    )
                    .when(self.advanced_open, |this| {
                        this.child(
                            div()
                                .v_flex()
                                .gap(px(4.0))
                                .child(div().text_label().text_color(p.muted).child(I18n::t(
                                    locale,
                                    "engine-match",
                                    "optionalSeed",
                                )))
                                .child(Input::new(&self.seed_input)),
                        )
                    }),
            )
            .when(
                !self.engines.is_empty() && !self.engine_available("gnubg"),
                |this| {
                    this.child(
                        StatusBanner::new(
                            "duel-no-gnubg",
                            BannerKind::Warning,
                            I18n::t(locale, "engine-match", "noEngines"),
                        )
                        .palette(p),
                    )
                },
            )
            .when_some(self.error.clone(), |this, err| {
                this.child(StatusBanner::new("duel-setup-error", BannerKind::Error, err).palette(p))
            })
    }
}

/// Complete GPUI Engine Duels workspace.
pub struct EngineMatchWorkspace {
    visible: bool,
    draft: SeriesDraft,
    engines: Vec<EngineInfo>,
    recent: Option<SeriesPage>,
    older_pages: Vec<SeriesPage>,
    selected_id: Option<Uuid>,
    offset: i32,
    details: Option<SeriesDetails>,
    loading: bool,
    loading_older: bool,
    action_busy: bool,
    opening_match: Option<i32>,
    open_epoch: u64,
    settings_expanded: bool,
    error_message: Option<SharedString>,
    status_notice: Option<SharedString>,
    _push_task: Option<Task<()>>,
    _load_task: Option<Task<()>>,
    _older_task: Option<Task<()>>,
    _action_task: Option<Task<()>>,
}

impl EngineMatchWorkspace {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut ws = Self {
            visible: false,
            draft: SeriesDraft::default(),
            engines: Vec::new(),
            recent: None,
            older_pages: Vec::new(),
            selected_id: None,
            offset: 0,
            details: None,
            loading: false,
            loading_older: false,
            action_busy: false,
            opening_match: None,
            open_epoch: 0,
            settings_expanded: false,
            error_message: None,
            status_notice: None,
            _push_task: None,
            _load_task: None,
            _older_task: None,
            _action_task: None,
        };
        ws.start_push_listener(cx);
        ws
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if !visible {
            self.open_epoch = self.open_epoch.wrapping_add(1);
            self.opening_match = None;
            return;
        }
        if self._push_task.is_none() {
            self.start_push_listener(cx);
        }
        self.reload_all(cx);
    }

    fn start_push_listener(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        if !cx.has_global::<TokioBridge>() {
            return;
        }
        let mut rx = facade.subscribe_engine_match_updates();
        let handle = TokioBridge::handle(cx);
        let task = cx.spawn(async move |this, cx| {
            loop {
                let recv = handle
                    .spawn(async move {
                        let res = rx.recv().await;
                        (rx, res)
                    })
                    .await;
                let Ok((next_rx, outcome)) = recv else {
                    break;
                };
                rx = next_rx;
                match outcome {
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let alive = this
                            .update(cx, |ws, cx| {
                                ws.reload_series_and_selected(cx);
                            })
                            .is_ok();
                        if !alive {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        self._push_task = Some(task);
    }

    fn effective_selected_id(&self) -> Option<Uuid> {
        self.selected_id.or_else(|| {
            self.recent
                .as_ref()
                .and_then(|p| p.runs.first().map(|r| r.id))
        })
    }

    fn reload_all(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let selected = self.effective_selected_id();
        let offset = self.offset;
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            let engines = facade.list_engines().await;
            let list = facade.list_engine_matches(None).await;
            let target_id = selected.or_else(|| {
                list.as_ref()
                    .ok()
                    .and_then(|p| p.runs.first().map(|r| r.id))
            });
            let details = match target_id {
                Some(id) => Some(facade.get_engine_match(id, offset).await),
                None => None,
            };
            (engines, list, details)
        }) else {
            return;
        };

        self.loading = true;
        self.error_message = None;
        cx.notify();

        self._load_task = Some(cx.spawn(async move |this, cx| {
            let Ok((eng_res, list_res, details_res)) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.loading = false;
                if let Ok(engines) = eng_res {
                    ws.engines = engines;
                }
                match list_res {
                    Ok(page) => {
                        ws.recent = Some(page);
                    }
                    Err(err) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                }
                if let Some(det) = details_res {
                    match det {
                        Ok(d) => {
                            ws.selected_id = Some(d.series.id);
                            ws.details = Some(d);
                        }
                        Err(err) => {
                            ws.error_message = Some(format_domain_error(locale, &err).into());
                        }
                    }
                }
                cx.notify();
            });
        }));
    }

    fn reload_series_and_selected(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let selected = self.effective_selected_id();
        let offset = self.offset;
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            let list = facade.list_engine_matches(None).await;
            let target_id = selected.or_else(|| {
                list.as_ref()
                    .ok()
                    .and_then(|p| p.runs.first().map(|r| r.id))
            });
            let details = match target_id {
                Some(id) => Some(facade.get_engine_match(id, offset).await),
                None => None,
            };
            (list, details)
        }) else {
            return;
        };

        self._load_task = Some(cx.spawn(async move |this, cx| {
            let Ok((list_res, details_res)) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                if let Ok(page) = list_res {
                    ws.recent = Some(page);
                }
                if let Some(Ok(d)) = details_res {
                    ws.selected_id = Some(d.series.id);
                    ws.details = Some(d);
                } else if let Some(Err(err)) = details_res {
                    ws.error_message = Some(format_domain_error(locale, &err).into());
                }
                cx.notify();
            });
        }));
    }

    fn select_series(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.effective_selected_id() == Some(id) && self.details.is_some() {
            return;
        }
        self.open_epoch = self.open_epoch.wrapping_add(1);
        self.opening_match = None;
        self.selected_id = Some(id);
        self.offset = 0;
        self.details = None;
        self.status_notice = None;
        cx.notify();
        self.reload_series_and_selected(cx);
    }

    fn load_older_series(&mut self, cx: &mut Context<Self>) {
        let cursor = self
            .older_pages
            .last()
            .and_then(|p| p.next_cursor)
            .or_else(|| self.recent.as_ref().and_then(|p| p.next_cursor));
        let Some(before) = cursor else { return };
        if self.loading_older {
            return;
        }
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            facade.list_engine_matches(Some(before)).await
        }) else {
            return;
        };
        self.loading_older = true;
        cx.notify();

        self._older_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.loading_older = false;
                match res {
                    Ok(page) => ws.older_pages.push(page),
                    Err(err) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn open_setup_modal(
        &mut self,
        prefill: Option<&SeriesConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = current_locale(cx);
        let (draft, advanced_open) = match prefill {
            Some(cfg) => (SeriesDraft::from_config(cfg), true),
            None => (self.draft.clone(), false),
        };
        let busy = ModalBusyLock::new(false);
        let parent = cx.entity().clone();
        let engines: Vec<DuelEngineOption> =
            self.engines.iter().map(DuelEngineOption::from).collect();
        let dialog_view = cx.new(|cx| {
            EngineSeriesSetupDialog::new(
                draft,
                engines,
                advanced_open,
                busy.clone(),
                parent,
                window,
                cx,
            )
        });
        let body_view = dialog_view.clone();
        let footer_view = dialog_view;

        open_modal_dialog(
            window,
            cx,
            ModalConfig::new(I18n::t(locale, "engine-match", "newSeries"))
                .size(ModalSize::Wide)
                .busy_lock(busy),
            move |_: &mut Window, _: &mut App| body_view.clone().into_any_element(),
            Some(move |_: &mut Window, cx: &mut App| {
                let p = NeonTheme::current_palette(cx);
                let loc = current_locale(cx);
                let dlg = footer_view.read(cx);
                let can_start = dlg.can_submit();
                let is_busy = dlg.busy.is_busy();
                let submit_view = footer_view.clone();
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .child(
                        ActionButton::new("duel-setup-cancel")
                            .icon(ActionIconName::Cancel)
                            .label(I18n::t(loc, "database-backup", "cancel"))
                            .disabled(is_busy)
                            .palette(p)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        ActionButton::new("duel-setup-start")
                            .icon(ActionIconName::Play)
                            .label(I18n::t(loc, "engine-match", "start"))
                            .primary()
                            .disabled(!can_start)
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                submit_view.update(cx, |d, cx| d.submit(window, cx));
                            }),
                    )
                    .into_any_element()
            }),
            None,
        );
    }

    fn cancel_selected(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.action_busy {
            return;
        }
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade.cancel_engine_match(id).await
        }) else {
            return;
        };
        self.action_busy = true;
        self.error_message = None;
        cx.notify();

        self._action_task = Some(cx.spawn(async move |this, cx| {
            let res = write_task.await;
            let _ = this.update(cx, |ws, cx| {
                ws.action_busy = false;
                match res {
                    Ok(Ok(_)) => ws.reload_series_and_selected(cx),
                    Ok(Err(err)) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                    Err(err) => {
                        ws.error_message = Some(err.to_string().into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn delete_selected(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.action_busy {
            return;
        }
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade.delete_engine_match(id).await
        }) else {
            return;
        };
        self.action_busy = true;
        self.error_message = None;
        cx.notify();

        self._action_task = Some(cx.spawn(async move |this, cx| {
            let res = write_task.await;
            let _ = this.update(cx, |ws, cx| {
                ws.action_busy = false;
                match res {
                    Ok(Ok(())) => {
                        if ws.selected_id == Some(id) {
                            ws.selected_id = None;
                            ws.details = None;
                        }
                        ws.reload_all(cx);
                    }
                    Ok(Err(err)) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                    Err(err) => {
                        ws.error_message = Some(err.to_string().into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn open_match_in_analysis(
        &mut self,
        series_id: Uuid,
        match_index: i32,
        cx: &mut Context<Self>,
    ) {
        if self.opening_match.is_some() {
            return;
        }
        let locale = current_locale(cx);
        self.open_epoch = self.open_epoch.wrapping_add(1);
        let epoch = self.open_epoch;
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade
                .engine_match_library_entry(series_id, match_index)
                .await
        }) else {
            return;
        };
        self.opening_match = Some(match_index);
        self.error_message = None;
        cx.notify();

        self._action_task = Some(cx.spawn(async move |this, cx| {
            let res = write_task.await;
            let _ = this.update(cx, |ws, cx| {
                if ws.open_epoch != epoch {
                    return;
                }
                ws.opening_match = None;
                match res {
                    Ok(Ok(summary)) => {
                        if ws.visible {
                            cx.emit(EngineMatchEvent::OpenMatchInAnalysis(summary.id));
                        }
                    }
                    Ok(Err(err)) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                    Err(err) => {
                        ws.error_message = Some(err.to_string().into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn export_match_sgf(&mut self, series_id: Uuid, match_index: i32, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = format!(
            "engine-duel-{}-{}.sgf",
            &series_id.to_string()[..8],
            match_index + 1
        );
        let prompt = cx.prompt_for_new_path(&dir, Some(&suggested));

        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dest_path))) = prompt.await else {
                return;
            };
            let fetch_task = cx.update(|cx| {
                spawn_facade_read(cx, move |facade| async move {
                    facade.engine_match_sgf(series_id, match_index).await
                })
            });
            let Some(fetch_task) = fetch_task else {
                return;
            };
            let res = fetch_task.await;
            let _ = this.update(cx, |ws, cx| {
                match res {
                    Ok(Ok(sgf)) => match std::fs::write(&dest_path, sgf.as_bytes()) {
                        Ok(()) => {
                            ws.status_notice =
                                Some(format!("SGF saved: {}", dest_path.display()).into());
                            ws.error_message = None;
                        }
                        Err(err) => {
                            ws.error_message = Some(err.to_string().into());
                        }
                    },
                    Ok(Err(err)) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                    Err(err) => {
                        ws.error_message = Some(err.to_string().into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn all_visible_runs(&self) -> Vec<Series> {
        let mut by_id: HashMap<Uuid, Series> = HashMap::new();
        let mut order: Vec<Uuid> = Vec::new();
        if let Some(recent) = &self.recent {
            for r in &recent.runs {
                order.push(r.id);
                by_id.insert(r.id, r.clone());
            }
        }
        for page in &self.older_pages {
            for r in &page.runs {
                by_id.entry(r.id).or_insert_with(|| {
                    order.push(r.id);
                    r.clone()
                });
            }
        }
        if let Some(det) = &self.details {
            let id = det.series.id;
            if !by_id.contains_key(&id) {
                order.insert(0, id);
            }
            by_id.insert(id, det.series.clone());
        }
        let mut rows: Vec<Series> = order
            .into_iter()
            .filter_map(|id| by_id.remove(&id))
            .collect();
        rows.sort_by_key(|r| !is_series_active(&r.status));
        rows
    }

    fn render_results(
        &self,
        det: &SeriesDetails,
        p: NeonPalette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let series = &det.series;
        let stats = &det.statistics;
        let config = &series.configuration;
        let paired = config.mode == DiceMode::Paired;
        let money_duel = config.format == DuelFormat::MoneyCubeless;
        let playing = is_series_active(&series.status);
        let counted = if paired {
            stats.complete_pairs * 2
        } else {
            series.completed_matches
        };
        let stopped_empty = !playing && counted == 0 && det.matches.is_empty();
        let rate_a = stats.win_rate_a;
        let rate_b = rate_a.map(|r| 1.0 - r);
        let entity = cx.entity().clone();
        let series_id = series.id;

        let progress_frac = if series.total_matches > 0 {
            (series.completed_matches as f32 / series.total_matches as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let mut header_actions = div().h_flex().items_center().gap(px(8.0));
        if playing {
            let ent = entity.clone();
            header_actions = header_actions.child(
                ActionButton::new("duel-cancel-btn")
                    .icon(ActionIconName::Cancel)
                    .label(I18n::t(locale, "engine-match", "cancel"))
                    .disabled(self.action_busy)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |ws, cx| ws.cancel_selected(series_id, cx));
                    }),
            );
        } else {
            let ent = entity.clone();
            header_actions = header_actions.child(
                ActionButton::new("duel-delete-btn")
                    .icon(ActionIconName::Delete)
                    .tooltip(I18n::t(locale, "benchmark", "confirmDelete"))
                    .danger()
                    .dense_nav()
                    .disabled(self.action_busy)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |ws, cx| ws.delete_selected(series_id, cx));
                    }),
            );
        }
        {
            let ent = entity.clone();
            let cfg_clone = config.clone();
            header_actions = header_actions.child(
                ActionButton::new("duel-reuse-btn")
                    .icon(ActionIconName::Copy)
                    .label(I18n::t(locale, "engine-match", "reuse"))
                    .palette(p)
                    .on_click(move |_, window, cx| {
                        ent.update(cx, |ws, cx| {
                            ws.open_setup_modal(Some(&cfg_clone), window, cx)
                        });
                    }),
            );
        }

        let per_side_cost = |fmt: fn(Locale, &SideCostSummary) -> String| {
            let a = stats
                .cost_a
                .as_ref()
                .map(|c| fmt(locale, c))
                .unwrap_or_else(|| "—".into());
            let b = stats
                .cost_b
                .as_ref()
                .map(|c| fmt(locale, c))
                .unwrap_or_else(|| "—".into());
            format!("A {a} · B {b}")
        };

        let mut out = div()
            .v_flex()
            .gap(px(12.0))
            // Progress & action bar
            .child(
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .tabular_nums()
                                    .text_color(p.text)
                                    .child(format!(
                                        "{} · {}",
                                        series_status_label(locale, &series.status),
                                        series_progress_text(locale, series)
                                    )),
                            )
                            .child(header_actions),
                    )
                    .when(playing, |this| {
                        this.child(
                            div()
                                .w_full()
                                .h(px(6.0))
                                .rounded(px(3.0))
                                .bg(p.border)
                                .overflow_hidden()
                                .child(
                                    div()
                                        .h_full()
                                        .w(gpui_kit::relative(progress_frac))
                                        .bg(p.accent),
                                ),
                        )
                    }),
            );

        if series.status == "failed" || series.error.is_some() {
            let mut msg = if series.status == "failed" {
                let mut s = I18n::t(locale, "engine-match", "failedSummary").to_string();
                if series.completed_matches > 0 {
                    s.push(' ');
                    s.push_str(I18n::t(locale, "engine-match", "partialResults"));
                }
                s
            } else {
                String::new()
            };
            if let Some(err) = &series.error {
                if !msg.is_empty() {
                    msg.push_str(" — ");
                }
                msg.push_str(err);
            }
            out = out
                .child(StatusBanner::new("duel-failure-banner", BannerKind::Error, msg).palette(p));
        }

        if !stopped_empty {
            // Win-rate cards + 2-segment proportional bar
            let win_card = |side_letter: &str, comp: &Competitor, rate: Option<f64>, wins: i32| {
                div()
                    .v_flex()
                    .gap(px(4.0))
                    .flex_1()
                    .p(px(12.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child(format!(
                                        "{side_letter} · {}",
                                        competitor_label(locale, comp)
                                    )),
                            )
                            .child(
                                div()
                                    .text_subject()
                                    .tabular_nums()
                                    .text_color(if side_letter == "A" { p.accent } else { p.text })
                                    .child(
                                        rate.map(|r| format_percent(locale, r, 1))
                                            .unwrap_or_else(|| "—".into()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .text_meta()
                            .tabular_nums()
                            .text_color(p.muted)
                            .child(I18n::t(
                                locale,
                                "engine-match",
                                if money_duel { "gamesWon" } else { "matchWins" },
                            ))
                            .child(format_int(locale, i64::from(wins))),
                    )
            };

            let mut win_section = div().v_flex().gap(px(8.0)).child(
                div()
                    .h_flex()
                    .gap(px(10.0))
                    .child(win_card("A", &config.a, rate_a, stats.wins_a))
                    .child(win_card("B", &config.b, rate_b, stats.wins_b)),
            );

            if let Some(r_a) = rate_a {
                let pct_a = r_a.clamp(0.0, 1.0) as f32;
                let pct_b = (1.0 - pct_a).clamp(0.0, 1.0);
                win_section = win_section.child(
                    div()
                        .id("duel-win-rate-bar")
                        .h_flex()
                        .w_full()
                        .h(px(10.0))
                        .rounded(px(5.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border)
                        .child(div().h_full().w(gpui_kit::relative(pct_a)).bg(p.accent))
                        .child(
                            div()
                                .h_full()
                                .w(gpui_kit::relative(pct_b))
                                .bg(p.border_strong),
                        ),
                );
            }
            out = out.child(win_section);

            // Statistics cards
            let stat_cell = |label: &str, value: String, sub: Option<String>| {
                div()
                    .v_flex()
                    .gap(px(3.0))
                    .min_w(px(190.0))
                    .flex_1()
                    .p(px(10.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .text_meta()
                            .text_color(p.muted)
                            .child(label.to_string()),
                    )
                    .child(
                        div()
                            .text_label()
                            .font_weight(FontWeight::SEMIBOLD)
                            .tabular_nums()
                            .text_color(p.text)
                            .child(value),
                    )
                    .when_some(sub, |this, s| {
                        this.child(
                            div()
                                .text_micro()
                                .tabular_nums()
                                .text_color(p.muted)
                                .child(s),
                        )
                    })
            };

            let mut stat_grid = div().h_flex().gap(px(8.0)).flex_wrap().child(stat_cell(
                I18n::t(
                    locale,
                    "engine-match",
                    if money_duel {
                        "blocksCounted"
                    } else {
                        "matchesCounted"
                    },
                ),
                format_int(locale, i64::from(counted)),
                stats.money.as_ref().map(|m| {
                    format!(
                        "{} {}",
                        I18n::t(locale, "engine-match", "games"),
                        format_int(locale, m.games)
                    )
                }),
            ));

            if paired {
                stat_grid = stat_grid.child(stat_cell(
                    I18n::t(locale, "engine-match", "completePairs"),
                    format!(
                        "{} / {}",
                        format_int(locale, i64::from(stats.complete_pairs)),
                        format_int(locale, i64::from(config.count))
                    ),
                    None,
                ));
            }

            if stats.cost_a.is_some() || stats.cost_b.is_some() {
                stat_grid = stat_grid.child(stat_cell(
                    I18n::t(locale, "engine-match", "cpuPerMove"),
                    per_side_cost(|loc, c| format_benchmark_duration(loc, c.cpu_ms_per_move)),
                    Some(format!(
                        "{} {}",
                        I18n::t(locale, "engine-match", "wallPerMove"),
                        per_side_cost(|loc, c| format_benchmark_duration(loc, c.wall_ms_per_move))
                    )),
                ));
            }

            if let Some(ci) = stats.confidence_interval_a {
                let ci_text = format!(
                    "{} – {}",
                    format_percent(locale, ci[0], 1),
                    format_percent(locale, ci[1], 1)
                );
                let verdict_key = if ci[0] <= 0.5 && ci[1] >= 0.5 {
                    "intervalInconclusive"
                } else {
                    "intervalClear"
                };
                stat_grid = stat_grid.child(stat_cell(
                    I18n::t(locale, "engine-match", "interval"),
                    ci_text,
                    Some(I18n::t(locale, "engine-match", verdict_key).to_string()),
                ));
            }

            if let Some(money) = &stats.money {
                if let Some(ppg) = money.points_per_game_a {
                    let sub = money.confidence_interval.map(|ci| {
                        let verdict = if ci[0] <= 0.0 && ci[1] >= 0.0 {
                            "ppgInconclusive"
                        } else {
                            "ppgClear"
                        };
                        format!(
                            "{}: {} – {} · {}",
                            I18n::t(locale, "engine-match", "interval"),
                            format_signed_decimal(locale, ci[0], 3, 3),
                            format_signed_decimal(locale, ci[1], 3, 3),
                            I18n::t(locale, "engine-match", verdict)
                        )
                    });
                    stat_grid = stat_grid.child(stat_cell(
                        I18n::t(locale, "engine-match", "pointsPerGame"),
                        format_signed_decimal(locale, ppg, 3, 3),
                        sub,
                    ));
                }
                if money.games > 0 {
                    let share =
                        |part: i64| format_percent(locale, part as f64 / money.games as f64, 1);
                    stat_grid = stat_grid.child(stat_cell(
                        I18n::t(locale, "engine-match", "gammonShare"),
                        format!(
                            "A {} / {} · B {} / {}",
                            share(money.outcomes_a.gammon),
                            share(money.outcomes_a.backgammon),
                            share(money.outcomes_b.gammon),
                            share(money.outcomes_b.backgammon),
                        ),
                        None,
                    ));
                }
            }

            out = out.child(stat_grid);
        }

        // Collapsible series settings
        {
            let ent = entity.clone();
            let open = self.settings_expanded;
            out = out.child(
                div()
                    .v_flex()
                    .gap(px(6.0))
                    .p(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        ActionButton::new("duel-settings-toggle")
                            .icon(if open {
                                ActionIconName::Collapse
                            } else {
                                ActionIconName::Expand
                            })
                            .label(I18n::t(locale, "engine-match", "seriesSettings"))
                            .ghost()
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent.update(cx, |ws, cx| {
                                    ws.settings_expanded = !ws.settings_expanded;
                                    cx.notify();
                                });
                            }),
                    )
                    .when(open, |this| {
                        this.child(
                            div()
                                .v_flex()
                                .gap(px(3.0))
                                .text_meta()
                                .text_color(p.muted)
                                .child(if money_duel {
                                    format!(
                                        "{} · {} · {} {}",
                                        I18n::t(locale, "engine-match", "formatMoney"),
                                        I18n::t(
                                            locale,
                                            "engine-match",
                                            if paired { "paired" } else { "random" }
                                        ),
                                        I18n::t(locale, "engine-match", "blockGames"),
                                        format_int(
                                            locale,
                                            i64::from(config.block_games.unwrap_or(0))
                                        )
                                    )
                                } else {
                                    format!(
                                        "{} · {} {}",
                                        I18n::t(
                                            locale,
                                            "engine-match",
                                            if paired { "paired" } else { "random" }
                                        ),
                                        I18n::t(locale, "engine-match", "length"),
                                        format_int(locale, i64::from(config.match_length))
                                    )
                                })
                                .child(div().mono_font().child(format!(
                                    "{}: {} · {}",
                                    I18n::t(locale, "engine-match", "seed"),
                                    config.seed,
                                    series.dice_version
                                )))
                                .child(format!(
                                    "{}: {}",
                                    I18n::t(locale, "engine-match", "engineA"),
                                    series
                                        .engines
                                        .a
                                        .engine_version
                                        .as_deref()
                                        .unwrap_or(&competitor_label(locale, &config.a))
                                ))
                                .child(format!(
                                    "{}: {}",
                                    I18n::t(locale, "engine-match", "engineB"),
                                    series
                                        .engines
                                        .b
                                        .engine_version
                                        .as_deref()
                                        .unwrap_or(&competitor_label(locale, &config.b))
                                )),
                        )
                    }),
            );
        }

        // Matches table
        if !stopped_empty {
            if det.matches.is_empty() {
                out = out.child(div().text_body().text_color(p.muted).child(I18n::t(
                    locale,
                    "engine-match",
                    "noMatches",
                )));
            } else {
                let mut table = div()
                    .v_flex()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(10.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .bg(p.surface_alt)
                            .border_b_1()
                            .border_color(p.border)
                            .text_table_heading()
                            .text_color(p.muted)
                            .child(div().w(px(120.0)).child(I18n::t(
                                locale,
                                "engine-match",
                                series_unit_key(money_duel, paired),
                            )))
                            .child(div().w(px(55.0)).child(I18n::t(
                                locale,
                                "engine-match",
                                "white",
                            )))
                            .child(div().w(px(90.0)).child(I18n::t(
                                locale,
                                "engine-match",
                                if money_duel { "points" } else { "score" },
                            )))
                            .child(div().w(px(65.0)).child(I18n::t(
                                locale,
                                "engine-match",
                                "games",
                            )))
                            .child(
                                div()
                                    .flex_1()
                                    .child(I18n::t(locale, "engine-match", "seed")),
                            )
                            .child(div().w(px(180.0)).child(I18n::t(
                                locale,
                                "engine-match",
                                "actions",
                            ))),
                    );

                for m in &det.matches {
                    let idx = m.match_index;
                    let is_return = paired && idx % 2 == 1;
                    let incomplete = paired && !is_return && idx + 1 == series.completed_matches;
                    let unit_num = if paired { (idx / 2) + 1 } else { idx + 1 };
                    let mut unit_label = format_int(locale, i64::from(unit_num));
                    if paired {
                        unit_label.push_str(" · ");
                        unit_label.push_str(I18n::t(
                            locale,
                            "engine-match",
                            if is_return { "return" } else { "outbound" },
                        ));
                    }
                    if incomplete {
                        unit_label.push_str(" (");
                        unit_label.push_str(I18n::t(locale, "engine-match", "incompletePair"));
                        unit_label.push(')');
                    }

                    let ent_open = entity.clone();
                    let ent_sgf = entity.clone();

                    table = table.child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(10.0))
                            .px(px(10.0))
                            .py(px(6.0))
                            .border_b_1()
                            .border_color(p.border)
                            .text_table()
                            .text_color(p.text)
                            .child(div().w(px(120.0)).child(unit_label))
                            .child(div().w(px(55.0)).child(if is_return { "B" } else { "A" }))
                            .child(div().w(px(90.0)).child(format!(
                                "{}–{}",
                                format_int(locale, i64::from(m.score_a)),
                                format_int(locale, i64::from(m.score_b))
                            )))
                            .child(
                                div()
                                    .w(px(65.0))
                                    .child(format_int(locale, i64::from(m.games))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .mono_font()
                                    .text_meta()
                                    .text_color(p.muted)
                                    .child(m.seed.to_string()),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .w(px(180.0))
                                    .child(
                                        ActionButton::new(SharedString::from(format!(
                                            "duel-open-{idx}"
                                        )))
                                        .icon(ActionIconName::Analysis)
                                        .label(I18n::t(locale, "engine-match", "analysis"))
                                        .tooltip(I18n::t(locale, "engine-match", "openInAnalysis"))
                                        .disabled(self.opening_match.is_some())
                                        .palette(p)
                                        .on_click(
                                            move |_, _, cx| {
                                                ent_open.update(cx, |ws, cx| {
                                                    ws.open_match_in_analysis(series_id, idx, cx);
                                                });
                                            },
                                        ),
                                    )
                                    .child(
                                        ActionButton::new(SharedString::from(format!(
                                            "duel-sgf-{idx}"
                                        )))
                                        .icon(ActionIconName::Export)
                                        .label("SGF")
                                        .palette(p)
                                        .on_click(
                                            move |_, _, cx| {
                                                ent_sgf.update(cx, |ws, cx| {
                                                    ws.export_match_sgf(series_id, idx, cx);
                                                });
                                            },
                                        ),
                                    ),
                            ),
                    );
                }
                out = out.child(table);
            }

            if det.previous_offset.is_some() || det.next_offset.is_some() {
                let prev = det.previous_offset;
                let next = det.next_offset;
                let ent_prev = entity.clone();
                let ent_next = entity.clone();
                out = out.child(
                    div()
                        .h_flex()
                        .gap(px(8.0))
                        .child(
                            ActionButton::new("duel-prev-page")
                                .icon(ActionIconName::Back)
                                .label(I18n::t(locale, "engine-match", "previous"))
                                .disabled(prev.is_none())
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    if let Some(off) = prev {
                                        ent_prev.update(cx, |ws, cx| {
                                            ws.offset = off;
                                            ws.reload_series_and_selected(cx);
                                        });
                                    }
                                }),
                        )
                        .child(
                            ActionButton::new("duel-next-page")
                                .icon(ActionIconName::Next)
                                .label(I18n::t(locale, "engine-match", "next"))
                                .disabled(next.is_none())
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    if let Some(off) = next {
                                        ent_next.update(cx, |ws, cx| {
                                            ws.offset = off;
                                            ws.reload_series_and_selected(cx);
                                        });
                                    }
                                }),
                        ),
                );
            }
        }

        out
    }
}

impl EventEmitter<EngineMatchEvent> for EngineMatchWorkspace {}

impl Render for EngineMatchWorkspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let entity = cx.entity().clone();
        let rows = self.all_visible_runs();
        let active_id = self.effective_selected_id();
        let has_older = self
            .older_pages
            .last()
            .and_then(|pg| pg.next_cursor)
            .or_else(|| self.recent.as_ref().and_then(|pg| pg.next_cursor))
            .is_some();

        // Left pane: series archive
        let mut list_box = div()
            .v_flex()
            .gap(px(8.0))
            .w(px(300.0))
            .flex_shrink_0()
            .p(px(12.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(div().text_panel().text_color(p.text).child(I18n::t(
                        locale,
                        "engine-match",
                        "series",
                    )))
                    .child({
                        let ent = entity.clone();
                        ActionButton::new("duel-new-series")
                            .icon(ActionIconName::Add)
                            .label(I18n::t(locale, "engine-match", "newSeries"))
                            .primary()
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                ent.update(cx, |ws, cx| ws.open_setup_modal(None, window, cx));
                            })
                    }),
            );

        if self.loading && rows.is_empty() {
            list_box = list_box.child(div().text_body().text_color(p.muted).child(I18n::t(
                locale,
                "engine-match",
                "loading",
            )));
        } else if rows.is_empty() {
            list_box = list_box.child(div().text_body().text_color(p.muted).child(I18n::t(
                locale,
                "engine-match",
                "empty",
            )));
        } else {
            let mut items = div().v_flex().gap(px(4.0));
            for run in rows {
                let run_id = run.id;
                let selected = active_id == Some(run_id);
                let ent = entity.clone();
                let mut status_line = series_status_label(locale, &run.status);
                if is_series_active(&run.status) {
                    status_line.push_str(" · ");
                    status_line.push_str(&series_progress_text(locale, &run));
                }
                items = items.child(
                    div()
                        .id(SharedString::from(format!("duel-row-{run_id}")))
                        .v_flex()
                        .gap(px(2.0))
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(if selected { p.accent } else { p.border })
                        .bg(if selected { p.selected } else { p.surface_alt })
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            ent.update(cx, |ws, cx| ws.select_series(run_id, cx));
                        })
                        .child(
                            div()
                                .text_micro()
                                .tabular_nums()
                                .text_color(p.muted)
                                .child(format_datetime_short_local(locale, run.created_at)),
                        )
                        .child(
                            div()
                                .text_label()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.text)
                                .child(format!(
                                    "{} / {}",
                                    competitor_label(locale, &run.configuration.a),
                                    competitor_label(locale, &run.configuration.b)
                                )),
                        )
                        .child(div().text_meta().text_color(p.muted).child(status_line)),
                );
            }
            list_box = list_box.child(items);
        }

        if has_older {
            let ent = entity.clone();
            list_box = list_box.child(
                ActionButton::new("duel-load-older")
                    .icon(ActionIconName::Expand)
                    .label(I18n::t(locale, "engine-match", "olderSeries"))
                    .disabled(self.loading_older)
                    .palette(p)
                    .on_click(move |_, _, cx| {
                        ent.update(cx, |ws, cx| ws.load_older_series(cx));
                    }),
            );
        }

        // Right pane: series results
        let right_pane = div()
            .v_flex()
            .gap(px(10.0))
            .flex_1()
            .min_w(px(0.0))
            .child(div().text_panel().text_color(p.text).child(I18n::t(
                locale,
                "engine-match",
                "results",
            )))
            .child(match &self.details {
                Some(det) if Some(det.series.id) == active_id => {
                    self.render_results(det, p, locale, cx).into_any_element()
                }
                _ if active_id.is_some() => div()
                    .text_body()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "engine-match", "loading"))
                    .into_any_element(),
                _ => div()
                    .text_body()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "engine-match", "empty"))
                    .into_any_element(),
            });

        let retry_ent = entity.clone();

        div()
            .id("engine-match-workspace")
            .test_support()
            .v_flex()
            .gap(px(12.0))
            .when_some(self.error_message.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("duel-error-banner", BannerKind::Error, msg)
                        .palette(p)
                        .on_retry(I18n::t(locale, "engine-match", "retry"), move |_, _, cx| {
                            retry_ent.update(cx, |ws, cx| ws.reload_all(cx));
                        }),
                )
            })
            .when_some(self.status_notice.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("duel-notice-banner", BannerKind::Success, msg).palette(p),
                )
            })
            .child(
                div()
                    .h_flex()
                    .items_start()
                    .gap(px(16.0))
                    .child(list_box)
                    .child(right_pane),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn student_t_975_and_ci_95_match_statistical_reference_values() {
        // Known exact 95% two-sided t critical values: df=19 -> 2.0930, df=29 -> 2.0452
        assert!((student_t_975(19) - 2.0930).abs() < 1e-3);
        assert!((student_t_975(29) - 2.0452).abs() < 1e-3);
        assert!((student_t_975(1000) - 1.9623).abs() < 1e-3);

        // Fewer than 20 observations returns None when min_samples = 20.
        let small = vec![0.5; 19];
        assert_eq!(
            compute_student_t_ci_95(&small, 0.0, 1.0, MIN_UNITS_FOR_CI),
            None
        );

        // 20 paired observations alternating 0.5 and 1.0 (mean = 0.75, s ≈ 0.25649).
        let obs: Vec<f64> = (0..20)
            .map(|i| if i % 2 == 0 { 0.5 } else { 1.0 })
            .collect();
        let [lo, hi] = compute_student_t_ci_95(&obs, 0.0, 1.0, MIN_UNITS_FOR_CI).unwrap();
        assert!(lo > 0.62 && lo < 0.64, "lo={lo}");
        assert!(hi > 0.86 && hi < 0.88, "hi={hi}");
        assert!(lo > 0.5, "excludes 50%");

        // Clamping to [0.0, 1.0] works on extreme samples.
        let extreme = vec![1.0; 20];
        assert_eq!(
            compute_student_t_ci_95(&extreme, 0.0, 1.0, MIN_UNITS_FOR_CI),
            Some([1.0, 1.0])
        );
    }

    #[test]
    fn series_draft_validates_point_money_and_uuid_seed_and_prefills_repeat() {
        let mut draft = SeriesDraft::default();
        assert!(draft.to_create_request().is_ok());

        draft.match_length = "26".into();
        assert!(draft.to_create_request().is_err());

        draft.format = DuelFormat::MoneyCubeless;
        draft.block_games = "100".into();
        draft.count = "10".into();
        draft.mode = DiceMode::Paired;
        assert_eq!(draft.total_money_games(), Some(2000));

        draft.seed = "not-a-uuid".into();
        assert!(draft.to_create_request().is_err());

        let seed = Uuid::new_v4();
        draft.seed = seed.to_string();
        let req = draft.to_create_request().unwrap();
        assert_eq!(req.seed, Some(seed));
        assert_eq!(req.block_games, Some(100));
        assert_eq!(req.match_length, 0);

        let cfg = SeriesConfig {
            a: req.a,
            b: req.b,
            format: DuelFormat::MoneyCubeless,
            match_length: 0,
            block_games: Some(250),
            count: 4,
            mode: DiceMode::Random,
            seed,
        };
        let repeated = SeriesDraft::from_config(&cfg);
        assert_eq!(repeated.block_games, "250");
        assert_eq!(repeated.count, "4");
        assert_eq!(repeated.seed, seed.to_string());
        assert_eq!(repeated.mode, DiceMode::Random);
    }
}
