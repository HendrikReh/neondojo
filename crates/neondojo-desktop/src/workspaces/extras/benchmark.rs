//! Extras → Engine Benchmark (`BenchmarkWorkspace.tsx`, `BenchmarkComparison.tsx`,
//! `BenchmarkHistoryTable.tsx`, `BenchmarkSetupForm.tsx`).
//!
//! Features:
//! - Analysis vs Play-turn setup with per-engine remembered depth (`GNUbg` / `NeonGammon`)
//! - Live push updates via `facade.subscribe_benchmark_updates()`
//! - `BenchmarkComparison` computing median + min–max once >= 3 matching completed runs exist
//! - 1,000-run sortable `BenchmarkHistoryTable` with expandable hardware/SHA-256 details
//!   and inline delete confirmation
//! - Emits [`BenchmarkEvent::BusyChanged`] while queued or running

use crate::app_context::DesktopAppContext;
use crate::i18n::{current_locale, format_domain_error};
use crate::theme::{NeonPalette, NeonTheme, NeonTypographyExt};
use crate::tokio_bridge::TokioBridge;
use crate::ui::action_button::{ActionButton, ActionButtonVariant};
use crate::ui::banners::{BannerKind, StatusBanner};
use crate::ui::icons::ActionIconName;
use crate::ui::text_nav::{TextNavBar, TextNavDensity, TextNavItem, TextNavOrientation};
use crate::workspaces::extras::{engine_display_name, spawn_facade_read, spawn_facade_write};
use gpui_kit::{
    Context, EventEmitter, FontWeight, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement, Styled, Task, TestSupportExt, Window,
    base::StyledExt, div, prelude::FluentBuilder, px,
};
use neondojo::benchmarks::{
    BenchmarkReference, BenchmarkRequest, BenchmarkRun, PlayBenchmarkRequest,
};
use neondojo::settings::EngineInfo;
use neondojo_i18n::{
    I18n, Locale, Param, compare_locale_base, format_benchmark_duration, format_benchmark_ms,
    format_datetime_short_local, format_decimal, format_int, format_ply,
};
use neondojo_rules::options::engine_depths;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

/// Events emitted by [`BenchmarkWorkspace`] to [`super::ExtrasWorkspace`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkEvent {
    BusyChanged(bool),
}

/// Measurement mode in the benchmark setup form.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkMode {
    #[default]
    Analysis,
    Play,
}

impl BenchmarkMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::Play => "play",
        }
    }
}

/// Sortable columns of the benchmark history table.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BenchmarkHistorySort {
    #[default]
    Date,
    Engine,
    Plies,
    Total,
    Average,
    Status,
}

/// Sort direction for the benchmark history table.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Asc,
    #[default]
    Desc,
}

/// Active sort column + direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BenchmarkHistoryOrder {
    pub sort: BenchmarkHistorySort,
    pub direction: SortDirection,
}

pub const DEFAULT_BENCHMARK_HISTORY_ORDER: BenchmarkHistoryOrder = BenchmarkHistoryOrder {
    sort: BenchmarkHistorySort::Date,
    direction: SortDirection::Desc,
};

/// Dates start newest first; other columns start ascending.
pub fn next_benchmark_history_order(
    current: BenchmarkHistoryOrder,
    sort: BenchmarkHistorySort,
) -> BenchmarkHistoryOrder {
    if current.sort == sort {
        BenchmarkHistoryOrder {
            sort,
            direction: match current.direction {
                SortDirection::Asc => SortDirection::Desc,
                SortDirection::Desc => SortDirection::Asc,
            },
        }
    } else {
        BenchmarkHistoryOrder {
            sort,
            direction: if sort == BenchmarkHistorySort::Date {
                SortDirection::Desc
            } else {
                SortDirection::Asc
            },
        }
    }
}

fn status_rank(status: &str) -> usize {
    match status {
        "running" => 0,
        "queued" => 1,
        "completed" => 2,
        "failed" => 3,
        "cancelled" => 4,
        _ => 5,
    }
}

fn measured_time(run: &BenchmarkRun, value: Option<f64>) -> Option<f64> {
    if run.status == "completed" {
        value.filter(|v| v.is_finite())
    } else {
        None
    }
}

/// Sorts one measurement type's history (`analysis` or `play`).
/// Runs without a measured time stay at the end in either direction, ties preserve
/// server newest-first order, and play mode maps `Total` sort back to `Date`.
pub fn sort_benchmark_runs(
    runs: &[BenchmarkRun],
    order: BenchmarkHistoryOrder,
    play: bool,
    locale: Locale,
) -> Vec<BenchmarkRun> {
    let effective = if play && order.sort == BenchmarkHistorySort::Total {
        DEFAULT_BENCHMARK_HISTORY_ORDER
    } else {
        order
    };
    let mut indexed: Vec<(usize, &BenchmarkRun)> = runs.iter().enumerate().collect();
    indexed.sort_by(|(idx_a, a), (idx_b, b)| {
        let ord = match effective.sort {
            BenchmarkHistorySort::Date => a.created_at.cmp(&b.created_at),
            BenchmarkHistorySort::Engine => {
                compare_locale_base(locale, &a.engine_name, &b.engine_name)
            }
            BenchmarkHistorySort::Plies => a.plies.cmp(&b.plies),
            BenchmarkHistorySort::Status => status_rank(&a.status).cmp(&status_rank(&b.status)),
            BenchmarkHistorySort::Total => {
                let va = measured_time(a, a.total_elapsed_ms);
                let vb = measured_time(b, b.total_elapsed_ms);
                return compare_optional_f64(va, vb, effective.direction, *idx_a, *idx_b);
            }
            BenchmarkHistorySort::Average => {
                let va = measured_time(
                    a,
                    if play {
                        a.average_turn_ms
                    } else {
                        a.average_move_ms
                    },
                );
                let vb = measured_time(
                    b,
                    if play {
                        b.average_turn_ms
                    } else {
                        b.average_move_ms
                    },
                );
                return compare_optional_f64(va, vb, effective.direction, *idx_a, *idx_b);
            }
        };
        let directed = match effective.direction {
            SortDirection::Asc => ord,
            SortDirection::Desc => ord.reverse(),
        };
        directed.then_with(|| idx_a.cmp(idx_b))
    });
    indexed.into_iter().map(|(_, run)| run.clone()).collect()
}

fn compare_optional_f64(
    va: Option<f64>,
    vb: Option<f64>,
    direction: SortDirection,
    idx_a: usize,
    idx_b: usize,
) -> Ordering {
    match (va, vb) {
        (None, None) => idx_a.cmp(&idx_b),
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => {
            let ord = a.total_cmp(&b);
            let directed = match direction {
                SortDirection::Asc => ord,
                SortDirection::Desc => ord.reverse(),
            };
            directed.then_with(|| idx_a.cmp(&idx_b))
        }
    }
}

/// Comparison aggregate over the 3 most recent identical completed benchmark runs.
#[derive(Debug, Clone)]
pub struct BenchmarkComparisonRow {
    pub run: BenchmarkRun,
    pub count: usize,
    pub median: f64,
    pub minimum: f64,
    pub maximum: f64,
}

/// Groups completed runs with identical machine, engine snapshot, ply, and protocol,
/// emitting median + min–max once at least 3 matching runs exist (using the 3 most
/// recently finished runs, matching `benchmark-comparison.ts`).
pub fn benchmark_comparisons(runs: &[BenchmarkRun]) -> Vec<BenchmarkComparisonRow> {
    let mut groups: BTreeMap<String, Vec<&BenchmarkRun>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();

    for run in runs {
        let value = if run.play_reference.is_some() {
            run.average_turn_ms
        } else {
            run.average_move_ms
        };
        let Some(value) = value else { continue };
        if run.status != "completed" || !value.is_finite() || value <= 0.0 {
            continue;
        }
        let Ok(key) = serde_json::to_string(&(
            &run.machine,
            &run.engine_id,
            &run.engine_version,
            &run.engine_path,
            &run.engine_snapshot,
            run.plies,
            &run.reference,
            &run.protocol,
            &run.play_reference,
        )) else {
            continue;
        };
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(run);
    }

    let mut out = Vec::new();
    for key in order {
        let Some(mut group) = groups.remove(&key) else {
            continue;
        };
        if group.len() < 3 {
            continue;
        }
        group.sort_by_key(|a| std::cmp::Reverse(a.finished_at));
        let sample = &group[..3];
        let mut times: Vec<f64> = sample
            .iter()
            .filter_map(|run| {
                if run.play_reference.is_some() {
                    run.average_turn_ms
                } else {
                    run.average_move_ms
                }
            })
            .collect();
        if times.len() < 3 {
            continue;
        }
        times.sort_by(f64::total_cmp);
        out.push(BenchmarkComparisonRow {
            run: sample[0].clone(),
            count: 3,
            minimum: times[0],
            median: times[1],
            maximum: times[2],
        });
    }
    out
}

/// Extracts recorded thread count from a benchmark run snapshot (or legacy version string).
pub fn benchmark_thread_count(run: &BenchmarkRun) -> Option<i32> {
    let engine = run.engine_snapshot.as_ref().map(|s| &s.engine);
    if run.engine_id != "bgsage" {
        return engine.and_then(|e| e.gnubg_threads);
    }
    if let Some(threads) = engine
        .and_then(|e| e.open_sage.as_ref())
        .and_then(|s| i32::try_from(s.threads).ok())
    {
        return Some(threads);
    }
    // Legacy format: "...; threads 4; ..."
    for part in run.engine_version.split(';') {
        if let Some(rest) = part.trim().strip_prefix("threads ")
            && let Ok(n) = rest.trim().parse::<i32>()
        {
            return Some(n);
        }
    }
    None
}

fn preferred_depth_for_engine(engine_id: &str) -> i32 {
    if engine_id == "bgsage" { 2 } else { 3 }
}

fn resolve_preferred_depth(engine_id: &str, available: &[i32]) -> Option<i32> {
    let pref = preferred_depth_for_engine(engine_id);
    if available.contains(&pref) {
        Some(pref)
    } else {
        available.first().copied()
    }
}

fn benchmark_status_label(locale: Locale, status: &str) -> String {
    match status {
        "queued" | "running" | "completed" | "failed" | "cancelled" => {
            I18n::t(locale, "benchmark", status).to_string()
        }
        other => other.to_string(),
    }
}

fn benchmark_progress_label(locale: Locale, run: &BenchmarkRun) -> String {
    if let Some(play_ref) = &run.play_reference {
        I18n::format(
            locale,
            "benchmark",
            "playProgress",
            &[
                ("completed", Param::from(i64::from(run.completed_turns))),
                ("total", Param::from(i64::from(play_ref.total_turns))),
            ],
        )
    } else {
        I18n::format(
            locale,
            "benchmark",
            "progress",
            &[
                ("completed", Param::from(i64::from(run.completed_games))),
                (
                    "total",
                    Param::from(i64::from(
                        run.reference.as_ref().map_or(0, |r| r.total_games),
                    )),
                ),
                ("moves", Param::from(i64::from(run.analyzed_moves))),
                (
                    "totalMoves",
                    Param::from(i64::from(
                        run.reference.as_ref().map_or(0, |r| r.total_moves),
                    )),
                ),
            ],
        )
    }
}

/// Complete GPUI Engine Benchmark sub-workspace.
pub struct BenchmarkWorkspace {
    visible: bool,
    mode: BenchmarkMode,
    reference: Option<BenchmarkReference>,
    engines: Vec<EngineInfo>,
    engine_id: String,
    default_analysis_plies: i32,
    default_play_plies: Option<i32>,
    analysis_depths_by_engine: HashMap<String, Vec<i32>>,
    play_depths_by_engine: HashMap<String, i32>,
    runs: Vec<BenchmarkRun>,
    queue_active: bool,
    loading: bool,
    starting: Option<BenchmarkMode>,
    cancelling: Option<Uuid>,
    deleting: Option<Uuid>,
    confirming_delete: Option<Uuid>,
    expanded_details: HashSet<Uuid>,
    expanded_comparison: HashSet<Uuid>,
    methodology_open: bool,
    history_order: BenchmarkHistoryOrder,
    error_message: Option<SharedString>,
    _load_task: Option<Task<()>>,
    _history_task: Option<Task<()>>,
    _action_task: Option<Task<()>>,
    _push_task: Option<Task<()>>,
}

impl BenchmarkWorkspace {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let reference = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
            .map(|f| f.benchmark_reference());

        let mut ws = Self {
            visible: false,
            mode: BenchmarkMode::Analysis,
            reference,
            engines: Vec::new(),
            engine_id: "gnubg".to_string(),
            default_analysis_plies: 3,
            default_play_plies: None,
            analysis_depths_by_engine: HashMap::new(),
            play_depths_by_engine: HashMap::new(),
            runs: Vec::new(),
            queue_active: false,
            loading: false,
            starting: None,
            cancelling: None,
            deleting: None,
            confirming_delete: None,
            expanded_details: HashSet::new(),
            expanded_comparison: HashSet::new(),
            methodology_open: false,
            history_order: DEFAULT_BENCHMARK_HISTORY_ORDER,
            error_message: None,
            _load_task: None,
            _history_task: None,
            _action_task: None,
            _push_task: None,
        };
        ws.start_push_listener(cx);
        ws
    }

    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        if visible {
            if self._push_task.is_none() {
                self.start_push_listener(cx);
            }
            self.reload_all(cx);
        }
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
        let mut rx = facade.subscribe_benchmark_updates();
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
                                ws.reload_history_only(cx);
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

    fn reload_all(&mut self, cx: &mut Context<Self>) {
        if let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        {
            self.reference = Some(facade.benchmark_reference());
        }
        let locale = current_locale(cx);
        let Some(read_task) = spawn_facade_read(cx, move |facade| async move {
            let engines = facade.list_engines().await;
            let history = facade.list_benchmarks().await;
            let cfg = facade.get_configuration().await.ok();
            (engines, history, cfg)
        }) else {
            return;
        };

        self.loading = true;
        self.error_message = None;
        cx.notify();

        self._load_task = Some(cx.spawn(async move |this, cx| {
            let Ok((engines_res, history_res, cfg)) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                ws.loading = false;
                if let Some(cfg) = cfg {
                    ws.default_analysis_plies = cfg.active_profile.configuration.analysis.plies;
                    ws.default_play_plies = Some(cfg.active_profile.configuration.play.plies);
                }
                match engines_res {
                    Ok(catalog) => {
                        let current_ok =
                            catalog.iter().any(|e| e.id == ws.engine_id && e.available);
                        if !current_ok && let Some(first) = catalog.iter().find(|e| e.available) {
                            ws.engine_id = first.id.clone();
                        }
                        ws.engines = catalog;
                    }
                    Err(err) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                }
                match history_res {
                    Ok(hist) => {
                        ws.runs = hist.runs;
                        ws.set_queue_active(hist.active, cx);
                    }
                    Err(err) => {
                        if ws.error_message.is_none() {
                            ws.error_message = Some(format_domain_error(locale, &err).into());
                        }
                    }
                }
                cx.notify();
            });
        }));
    }

    fn reload_history_only(&mut self, cx: &mut Context<Self>) {
        let locale = current_locale(cx);
        let Some(read_task) =
            spawn_facade_read(
                cx,
                move |facade| async move { facade.list_benchmarks().await },
            )
        else {
            return;
        };
        self._history_task = Some(cx.spawn(async move |this, cx| {
            let Ok(res) = read_task.await else {
                return;
            };
            let _ = this.update(cx, |ws, cx| {
                match res {
                    Ok(hist) => {
                        let was_active = ws.queue_active;
                        ws.runs = hist.runs;
                        ws.set_queue_active(hist.active, cx);
                        if was_active && !hist.active {
                            ws.reload_all(cx);
                            return;
                        }
                    }
                    Err(err) => {
                        ws.error_message = Some(format_domain_error(locale, &err).into());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn set_queue_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.queue_active != active {
            self.queue_active = active;
            cx.emit(BenchmarkEvent::BusyChanged(active));
        }
    }

    fn active_engine(&self) -> Option<&EngineInfo> {
        self.engines
            .iter()
            .find(|e| e.id == self.engine_id && e.available)
    }

    fn selected_analysis_plies(&self) -> (Vec<i32>, Option<i32>) {
        let requested = self
            .analysis_depths_by_engine
            .get(&self.engine_id)
            .cloned()
            .unwrap_or_else(|| vec![self.default_analysis_plies]);
        let available = self
            .active_engine()
            .map(|e| e.plies.as_slice())
            .unwrap_or(&[]);
        let mut compatible: Vec<i32> = requested
            .iter()
            .copied()
            .filter(|ply| available.contains(ply))
            .collect();
        compatible.sort_unstable();
        compatible.dedup();
        if !requested.is_empty()
            && compatible.is_empty()
            && let Some(fb) = resolve_preferred_depth(&self.engine_id, available)
        {
            return (vec![fb], Some(fb));
        }
        (compatible, None)
    }

    fn supported_play_depths(&self) -> Vec<i32> {
        let available = self
            .active_engine()
            .map(|e| e.plies.as_slice())
            .unwrap_or(&[]);
        engine_depths(&self.engine_id, "play")
            .into_iter()
            .filter(|ply| available.contains(ply))
            .collect()
    }

    fn selected_play_depth(&self) -> (Option<i32>, Option<i32>) {
        let supported = self.supported_play_depths();
        let requested = self
            .play_depths_by_engine
            .get(&self.engine_id)
            .copied()
            .or(self.default_play_plies)
            .unwrap_or_else(|| preferred_depth_for_engine(&self.engine_id));
        if supported.contains(&requested) {
            (Some(requested), None)
        } else {
            let fb = resolve_preferred_depth(&self.engine_id, &supported);
            (fb, fb)
        }
    }

    fn toggle_analysis_ply(&mut self, ply: i32, cx: &mut Context<Self>) {
        let (mut current, _) = self.selected_analysis_plies();
        if let Some(pos) = current.iter().position(|&p| p == ply) {
            current.remove(pos);
        } else {
            current.push(ply);
            current.sort_unstable();
        }
        self.analysis_depths_by_engine
            .insert(self.engine_id.clone(), current);
        cx.notify();
    }

    fn start_analysis(&mut self, cx: &mut Context<Self>) {
        let (plies, _) = self.selected_analysis_plies();
        if self.starting.is_some()
            || self.queue_active
            || self.active_engine().is_none()
            || plies.is_empty()
        {
            return;
        }
        let engine_id = self.engine_id.clone();
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade
                .start_benchmark(BenchmarkRequest { engine_id, plies })
                .await
        }) else {
            return;
        };
        self.starting = Some(BenchmarkMode::Analysis);
        self.error_message = None;
        cx.notify();

        self._action_task = Some(cx.spawn(async move |this, cx| {
            let res = write_task.await;
            let _ = this.update(cx, |ws, cx| {
                ws.starting = None;
                match res {
                    Ok(Ok(mut created)) => {
                        created.extend(ws.runs.clone());
                        ws.runs = created;
                        ws.set_queue_active(true, cx);
                        ws.reload_history_only(cx);
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

    fn start_play(&mut self, cx: &mut Context<Self>) {
        let (Some(plies), _) = self.selected_play_depth() else {
            return;
        };
        if self.starting.is_some() || self.queue_active || self.active_engine().is_none() {
            return;
        }
        let engine_id = self.engine_id.clone();
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade
                .start_play_benchmark(Some(PlayBenchmarkRequest { engine_id, plies }))
                .await
        }) else {
            return;
        };
        self.starting = Some(BenchmarkMode::Play);
        self.error_message = None;
        cx.notify();

        self._action_task = Some(cx.spawn(async move |this, cx| {
            let res = write_task.await;
            let _ = this.update(cx, |ws, cx| {
                ws.starting = None;
                match res {
                    Ok(Ok(created)) => {
                        ws.runs.insert(0, created);
                        ws.set_queue_active(true, cx);
                        ws.reload_history_only(cx);
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

    fn cancel_run(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.cancelling.is_some() {
            return;
        }
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade.cancel_benchmark(id).await
        }) else {
            return;
        };
        self.cancelling = Some(id);
        self.error_message = None;
        cx.notify();

        self._action_task = Some(cx.spawn(async move |this, cx| {
            let res = write_task.await;
            let _ = this.update(cx, |ws, cx| {
                ws.cancelling = None;
                match res {
                    Ok(Ok(_)) => ws.reload_history_only(cx),
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

    fn delete_run(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.deleting.is_some() {
            return;
        }
        let locale = current_locale(cx);
        let Some(write_task) = spawn_facade_write(cx, move |facade| async move {
            facade.delete_benchmark(id).await
        }) else {
            return;
        };
        self.deleting = Some(id);
        self.error_message = None;
        cx.notify();

        self._action_task = Some(cx.spawn(async move |this, cx| {
            let res = write_task.await;
            let _ = this.update(cx, |ws, cx| {
                ws.deleting = None;
                match res {
                    Ok(Ok(())) => {
                        ws.runs.retain(|r| r.id != id);
                        ws.confirming_delete = None;
                        ws.expanded_details.remove(&id);
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

    fn render_setup_panel(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let play = self.mode == BenchmarkMode::Play;
        let can_start_base = self.starting.is_none() && !self.queue_active && !self.loading;
        let (selected_analysis, analysis_fallback) = self.selected_analysis_plies();
        let (selected_play, play_fallback) = self.selected_play_depth();
        let supported_play = self.supported_play_depths();
        let active_engine = self.active_engine();
        let entity = cx.entity().clone();

        // Engine selector strip
        let mut engine_row = div().h_flex().items_center().gap(px(8.0)).flex_wrap();
        if self.engines.is_empty() {
            for id in ["gnubg", "bgsage"] {
                let is_sel = self.engine_id == id;
                let id_owned = id.to_string();
                let ent = entity.clone();
                engine_row = engine_row.child(
                    ActionButton::new(SharedString::from(format!("bench-engine-{id}")))
                        .label(engine_display_name(id))
                        .active(is_sel)
                        .disabled(self.starting.is_some())
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            let next = id_owned.clone();
                            ent.update(cx, |ws, cx| {
                                ws.engine_id = next;
                                cx.notify();
                            });
                        }),
                );
            }
        } else {
            for eng in &self.engines {
                let is_sel = self.engine_id == eng.id;
                let id_owned = eng.id.clone();
                let ent = entity.clone();
                let label = if eng.available {
                    eng.name.clone()
                } else {
                    format!(
                        "{} ({})",
                        eng.name,
                        I18n::t(locale, "engine-match", "unavailable")
                    )
                };
                engine_row = engine_row.child(
                    ActionButton::new(SharedString::from(format!("bench-engine-{}", eng.id)))
                        .label(label)
                        .active(is_sel)
                        .disabled(!eng.available || self.starting.is_some())
                        .palette(p)
                        .on_click(move |_, _, cx| {
                            let next = id_owned.clone();
                            ent.update(cx, |ws, cx| {
                                ws.engine_id = next;
                                cx.notify();
                            });
                        }),
                );
            }
        }

        let mode_nav = {
            let ent = entity.clone();
            TextNavBar::new(
                "benchmark-mode-nav",
                vec![
                    TextNavItem::new("analysis", I18n::t(locale, "benchmark", "matchAnalysis")),
                    TextNavItem::new("play", I18n::t(locale, "benchmark", "playLatency")),
                ],
            )
            .orientation(TextNavOrientation::Horizontal)
            .density(TextNavDensity::Compact)
            .active(Some(self.mode.as_str()))
            .palette(p)
            .on_select(move |id, _, cx| {
                let next = if id == "play" {
                    BenchmarkMode::Play
                } else {
                    BenchmarkMode::Analysis
                };
                ent.update(cx, |ws, cx| {
                    ws.mode = next;
                    cx.notify();
                });
            })
        };

        let controls =
            if play {
                let all_play = engine_depths("gnubg", "play");
                let mut ply_bar = div().h_flex().items_center().gap(px(6.0)).flex_wrap();
                for ply in all_play {
                    let supported = supported_play.contains(&ply);
                    let active = selected_play == Some(ply);
                    let ent = entity.clone();
                    ply_bar = ply_bar.child(
                        ActionButton::new(SharedString::from(format!("bench-play-ply-{ply}")))
                            .label(format_ply(locale, ply as u32))
                            .active(active)
                            .disabled(!supported || self.starting.is_some())
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent.update(cx, |ws, cx| {
                                    ws.play_depths_by_engine.insert(ws.engine_id.clone(), ply);
                                    cx.notify();
                                });
                            }),
                    );
                }
                let start_ent = entity.clone();
                let can_start_play =
                    can_start_base && selected_play.is_some() && active_engine.is_some();
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .flex_wrap()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(div().text_label().text_color(p.muted).child(I18n::t(
                                        locale,
                                        "benchmark",
                                        "playPly",
                                    )))
                                    .child(ply_bar),
                            )
                            .child(
                                ActionButton::new("benchmark-start-play")
                                    .icon(ActionIconName::Play)
                                    .label(I18n::t(
                                        locale,
                                        "benchmark",
                                        if self.starting == Some(BenchmarkMode::Play) {
                                            "starting"
                                        } else {
                                            "startPlay"
                                        },
                                    ))
                                    .primary()
                                    .disabled(!can_start_play)
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        start_ent.update(cx, |ws, cx| ws.start_play(cx));
                                    }),
                            ),
                    )
                    .when_some(play_fallback, |this, fb| {
                        this.child(div().text_meta().text_color(p.warning_text).child(
                            I18n::format(
                                locale,
                                "benchmark",
                                "depthFallback",
                                &[("ply", Param::from(i64::from(fb)))],
                            ),
                        ))
                    })
                    .child(div().text_meta().text_color(p.muted).child(I18n::t(
                        locale,
                        "benchmark",
                        "playTiming",
                    )))
                    .when_some(self.default_play_plies, |this, ply| {
                        this.child(div().text_meta().text_color(p.muted).child(I18n::format(
                            locale,
                            "benchmark",
                            "playDepth",
                            &[("ply", Param::from(i64::from(ply)))],
                        )))
                    })
            } else {
                let all_analysis = engine_depths("gnubg", "analysis");
                let available = active_engine.map(|e| e.plies.as_slice()).unwrap_or(&[]);
                let mut ply_bar = div().h_flex().items_center().gap(px(6.0)).flex_wrap();
                for ply in all_analysis {
                    let supported = available.contains(&ply);
                    let checked = selected_analysis.contains(&ply);
                    let ent = entity.clone();
                    ply_bar = ply_bar.child(
                        ActionButton::new(SharedString::from(format!("bench-analysis-ply-{ply}")))
                            .label(format_ply(locale, ply as u32))
                            .active(checked)
                            .disabled(!supported || self.starting.is_some())
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent.update(cx, |ws, cx| ws.toggle_analysis_ply(ply, cx));
                            }),
                    );
                }
                let start_ent = entity.clone();
                let can_start_analysis = can_start_base
                    && self.reference.is_some()
                    && active_engine.is_some()
                    && !selected_analysis.is_empty();
                div()
                    .v_flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .flex_wrap()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(div().text_label().text_color(p.muted).child(I18n::t(
                                        locale,
                                        "benchmark",
                                        "plies",
                                    )))
                                    .child(ply_bar),
                            )
                            .child(
                                ActionButton::new("benchmark-start-analysis")
                                    .icon(ActionIconName::Play)
                                    .label(I18n::t(
                                        locale,
                                        "benchmark",
                                        if self.starting == Some(BenchmarkMode::Analysis) {
                                            "starting"
                                        } else {
                                            "start"
                                        },
                                    ))
                                    .primary()
                                    .disabled(!can_start_analysis)
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        start_ent.update(cx, |ws, cx| ws.start_analysis(cx));
                                    }),
                            ),
                    )
                    .when_some(analysis_fallback, |this, fb| {
                        this.child(div().text_meta().text_color(p.warning_text).child(
                            I18n::format(
                                locale,
                                "benchmark",
                                "depthFallback",
                                &[("ply", Param::from(i64::from(fb)))],
                            ),
                        ))
                    })
                    .child(div().text_meta().text_color(p.muted).child(I18n::t(
                        locale,
                        "benchmark",
                        "serial",
                    )))
            };

        div()
            .v_flex()
            .gap(px(12.0))
            .p(px(16.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .flex_wrap()
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(div().text_label().text_color(p.muted).child(I18n::t(
                                locale,
                                "benchmark",
                                "engine",
                            )))
                            .child(engine_row),
                    )
                    .child(mode_nav),
            )
            .when(
                active_engine.is_none() && !self.engines.is_empty(),
                |this| {
                    this.child(div().text_meta().text_color(p.warning_text).child(I18n::t(
                        locale,
                        "benchmark",
                        "noEngine",
                    )))
                },
            )
            .child(controls)
            .when(self.queue_active, |this| {
                this.child(
                    StatusBanner::new(
                        "benchmark-queue-busy",
                        BannerKind::Info,
                        I18n::t(locale, "benchmark", "engineBusy"),
                    )
                    .palette(p),
                )
            })
    }

    fn render_active_runs(
        &self,
        p: NeonPalette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let active: Vec<&BenchmarkRun> = self
            .runs
            .iter()
            .filter(|r| r.status == "running" || r.status == "queued")
            .collect();
        if active.is_empty() {
            return None;
        }
        let entity = cx.entity().clone();
        let mut section = div()
            .v_flex()
            .gap(px(8.0))
            .p(px(14.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.accent)
            .bg(p.surface)
            .child(div().text_panel().text_color(p.text).child(I18n::t(
                locale,
                "benchmark",
                "active",
            )));

        for run in active {
            let run_id = run.id;
            let progress_text = benchmark_progress_label(locale, run);
            let max_val = run
                .play_reference
                .as_ref()
                .map(|r| r.total_turns)
                .or_else(|| run.reference.as_ref().map(|r| r.total_games))
                .unwrap_or(1)
                .max(1);
            let cur_val = if run.play_reference.is_some() {
                run.completed_turns
            } else {
                run.completed_games
            }
            .clamp(0, max_val);
            let frac = cur_val as f32 / max_val as f32;
            let ent = entity.clone();

            section = section.child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .p(px(10.0))
                    .rounded(px(6.0))
                    .bg(p.surface_alt)
                    .child(
                        div()
                            .v_flex()
                            .gap(px(4.0))
                            .flex_1()
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child(format!(
                                        "{} · {} · {} · {}",
                                        I18n::t(
                                            locale,
                                            "benchmark",
                                            if run.play_reference.is_some() {
                                                "playLatency"
                                            } else {
                                                "matchAnalysis"
                                            }
                                        ),
                                        run.engine_name,
                                        format_ply(locale, run.plies as u32),
                                        benchmark_status_label(locale, &run.status),
                                    )),
                            )
                            .child(
                                div()
                                    .text_meta()
                                    .tabular_nums()
                                    .text_color(p.muted)
                                    .child(progress_text),
                            )
                            .when(run.status == "queued" || run.started_at.is_none(), |this| {
                                this.child(div().text_meta().text_color(p.muted).child(I18n::t(
                                    locale,
                                    "benchmark",
                                    "waiting",
                                )))
                            })
                            .child(
                                div()
                                    .w_full()
                                    .h(px(6.0))
                                    .rounded(px(3.0))
                                    .bg(p.border)
                                    .overflow_hidden()
                                    .child(div().h_full().w(gpui_kit::relative(frac)).bg(p.accent)),
                            ),
                    )
                    .child(
                        ActionButton::new(SharedString::from(format!("bench-cancel-{run_id}")))
                            .icon(ActionIconName::Cancel)
                            .label(I18n::t(
                                locale,
                                "benchmark",
                                if self.cancelling == Some(run_id) {
                                    "cancelling"
                                } else {
                                    "cancel"
                                },
                            ))
                            .disabled(self.cancelling.is_some())
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent.update(cx, |ws, cx| ws.cancel_run(run_id, cx));
                            }),
                    ),
            );
        }

        Some(section)
    }

    fn render_comparison_section(
        &self,
        mode_runs: &[BenchmarkRun],
        p: NeonPalette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        let groups = benchmark_comparisons(mode_runs);
        if groups.is_empty() {
            return None;
        }
        let (heading, col_median, col_range) = match locale {
            Locale::De => ("Vergleich · Median aus 3 Läufen", "Median", "Spanne"),
            Locale::En => ("Comparison · median of 3 runs", "Median", "Range"),
        };
        let entity = cx.entity().clone();

        let mut table = div()
            .v_flex()
            .gap(px(4.0))
            .p(px(14.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(div().text_panel().text_color(p.text).child(heading))
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(p.border)
                    .text_table_heading()
                    .text_color(p.muted)
                    .child(
                        div()
                            .w(px(200.0))
                            .child(I18n::t(locale, "benchmark", "engineColumn")),
                    )
                    .child(
                        div()
                            .w(px(70.0))
                            .child(I18n::t(locale, "benchmark", "depth")),
                    )
                    .child(div().w(px(120.0)).child(col_median))
                    .child(div().w(px(180.0)).child(col_range))
                    .child(
                        div()
                            .flex_1()
                            .child(I18n::t(locale, "benchmark", "details")),
                    ),
            );

        for row in groups {
            let run = &row.run;
            let run_id = run.id;
            let expanded = self.expanded_comparison.contains(&run_id);
            let threads = benchmark_thread_count(run);
            let engine_cell = match threads {
                Some(n) => format!(
                    "{} · {}",
                    run.engine_name,
                    I18n::format(
                        locale,
                        "benchmark",
                        "threads",
                        &[("count", Param::from(i64::from(n)))]
                    )
                ),
                None => run.engine_name.clone(),
            };
            let range_str = format!(
                "{}–{}",
                format_decimal(locale, row.minimum, 2, 2),
                format_benchmark_ms(locale, row.maximum)
            );
            let ent = entity.clone();
            table = table.child(
                div()
                    .v_flex()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(8.0))
                            .py(px(6.0))
                            .text_table()
                            .text_color(p.text)
                            .child(
                                div()
                                    .w(px(200.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(engine_cell),
                            )
                            .child(
                                div()
                                    .w(px(70.0))
                                    .child(format_ply(locale, run.plies as u32)),
                            )
                            .child(
                                div()
                                    .w(px(120.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format_benchmark_ms(locale, row.median)),
                            )
                            .child(div().w(px(180.0)).text_color(p.muted).child(range_str))
                            .child(
                                ActionButton::new(SharedString::from(format!(
                                    "bench-cmp-details-{run_id}"
                                )))
                                .icon(if expanded {
                                    ActionIconName::Collapse
                                } else {
                                    ActionIconName::Expand
                                })
                                .label(I18n::t(locale, "benchmark", "details"))
                                .ghost()
                                .palette(p)
                                .on_click(move |_, _, cx| {
                                    ent.update(cx, |ws, cx| {
                                        if !ws.expanded_comparison.remove(&run_id) {
                                            ws.expanded_comparison.insert(run_id);
                                        }
                                        cx.notify();
                                    });
                                }),
                            ),
                    )
                    .when(expanded, |this| {
                        let protocol = run
                            .play_reference
                            .as_ref()
                            .map(|r| r.protocol_id.as_str())
                            .or_else(|| run.protocol.as_ref().map(|pr| pr.id.as_str()))
                            .unwrap_or("—");
                        this.child(
                            div()
                                .v_flex()
                                .gap(px(2.0))
                                .px(px(12.0))
                                .py(px(8.0))
                                .bg(p.surface_alt)
                                .text_meta()
                                .text_color(p.muted)
                                .child(format!(
                                    "{} · {} · {}",
                                    run.machine.name, run.machine.os, run.machine.architecture
                                ))
                                .child(format!(
                                    "{}: {}",
                                    I18n::t(locale, "benchmark", "recordedEngineVersion"),
                                    run.engine_version
                                ))
                                .child(format!(
                                    "{}: {protocol}",
                                    I18n::t(locale, "benchmark", "protocol")
                                )),
                        )
                    }),
            );
        }

        Some(table)
    }

    fn render_history_table(
        &self,
        mode_runs: &[BenchmarkRun],
        p: NeonPalette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let play = self.mode == BenchmarkMode::Play;
        let sorted = sort_benchmark_runs(mode_runs, self.history_order, play, locale);
        let entity = cx.entity().clone();

        let sort_header = |sort: BenchmarkHistorySort, label: &str, width_px: f32| {
            let active = self.history_order.sort == sort;
            let arrow = if active {
                match self.history_order.direction {
                    SortDirection::Asc => " ↑",
                    SortDirection::Desc => " ↓",
                }
            } else {
                ""
            };
            let ent = entity.clone();
            div()
                .id(SharedString::from(format!("bench-sort-{sort:?}")))
                .w(px(width_px))
                .cursor_pointer()
                .text_table_heading()
                .text_color(if active { p.accent } else { p.muted })
                .on_click(move |_, _, cx| {
                    ent.update(cx, |ws, cx| {
                        ws.history_order = next_benchmark_history_order(ws.history_order, sort);
                        cx.notify();
                    });
                })
                .child(format!("{label}{arrow}"))
        };

        let mut container = div()
            .v_flex()
            .gap(px(8.0))
            .p(px(14.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_panel().text_color(p.text).child(I18n::t(
                        locale,
                        "benchmark",
                        "history",
                    )))
                    .child(
                        div()
                            .text_meta()
                            .tabular_nums()
                            .text_color(p.muted)
                            .child(format_int(locale, sorted.len() as i64)),
                    ),
            );

        if self.loading && sorted.is_empty() {
            return container.child(div().text_body().text_color(p.muted).child(I18n::t(
                locale,
                "benchmark",
                "loading",
            )));
        }
        if sorted.is_empty() {
            return container.child(div().text_body().text_color(p.muted).child(I18n::t(
                locale,
                "benchmark",
                "empty",
            )));
        }

        // Table header row
        let mut header_row = div()
            .h_flex()
            .items_center()
            .gap(px(10.0))
            .px(px(8.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(p.border)
            .bg(p.surface_alt)
            .child(sort_header(
                BenchmarkHistorySort::Date,
                I18n::t(locale, "benchmark", "date"),
                145.0,
            ))
            .child(sort_header(
                BenchmarkHistorySort::Engine,
                I18n::t(locale, "benchmark", "engineColumn"),
                130.0,
            ))
            .child(sort_header(
                BenchmarkHistorySort::Plies,
                I18n::t(locale, "benchmark", "depth"),
                65.0,
            ));
        if !play {
            header_row = header_row.child(sort_header(
                BenchmarkHistorySort::Total,
                I18n::t(locale, "benchmark", "total"),
                125.0,
            ));
        }
        header_row = header_row
            .child(sort_header(
                BenchmarkHistorySort::Average,
                I18n::t(
                    locale,
                    "benchmark",
                    if play { "playAverage" } else { "average" },
                ),
                140.0,
            ))
            .child(sort_header(
                BenchmarkHistorySort::Status,
                I18n::t(locale, "benchmark", "status"),
                130.0,
            ))
            .child(
                div()
                    .flex_1()
                    .text_table_heading()
                    .text_color(p.muted)
                    .child(I18n::t(locale, "benchmark", "details")),
            );

        container = container.child(header_row);

        let mut rows_box = div().v_flex();
        for run in sorted {
            let run_id = run.id;
            let expanded = self.expanded_details.contains(&run_id);
            let confirming = self.confirming_delete == Some(run_id);
            let is_active = run.status == "queued" || run.status == "running";

            let total_str = if run.status == "completed" {
                run.total_elapsed_ms
                    .map(|ms| format_benchmark_duration(locale, ms))
                    .unwrap_or_else(|| "—".to_string())
            } else {
                "—".to_string()
            };
            let avg_raw = if play {
                run.average_turn_ms
            } else {
                run.average_move_ms
            };
            let avg_str = if run.status == "completed" {
                avg_raw
                    .map(|ms| format_benchmark_ms(locale, ms))
                    .unwrap_or_else(|| "—".to_string())
            } else {
                "—".to_string()
            };

            let status_color = match run.status.as_str() {
                "completed" => p.success_text,
                "failed" => p.danger_text,
                "running" | "queued" => p.accent,
                _ => p.muted,
            };

            let ent_expand = entity.clone();
            let ent_del = entity.clone();
            let ent_confirm = entity.clone();
            let ent_keep = entity.clone();

            let mut row_line = div()
                .h_flex()
                .items_center()
                .gap(px(10.0))
                .px(px(8.0))
                .py(px(6.0))
                .text_table()
                .text_color(p.text)
                .child(
                    div()
                        .w(px(145.0))
                        .text_color(p.muted)
                        .child(format_datetime_short_local(locale, run.created_at)),
                )
                .child(div().w(px(130.0)).child(run.engine_name.clone()))
                .child(
                    div()
                        .w(px(65.0))
                        .child(format_ply(locale, run.plies as u32)),
                );

            if !play {
                row_line = row_line.child(div().w(px(125.0)).child(total_str));
            }

            row_line = row_line
                .child(
                    div()
                        .w(px(140.0))
                        .font_weight(FontWeight::MEDIUM)
                        .child(avg_str),
                )
                .child(
                    div()
                        .v_flex()
                        .w(px(130.0))
                        .child(
                            div()
                                .text_color(status_color)
                                .font_weight(FontWeight::MEDIUM)
                                .child(benchmark_status_label(locale, &run.status)),
                        )
                        .when(run.status != "completed", |this| {
                            this.child(
                                div()
                                    .text_micro()
                                    .text_color(p.muted)
                                    .child(benchmark_progress_label(locale, &run)),
                            )
                        })
                        .when_some(run.error.clone(), |this, err_text| {
                            this.child(div().text_micro().text_color(p.danger_text).child(err_text))
                        }),
                )
                .child(
                    div()
                        .h_flex()
                        .items_center()
                        .justify_between()
                        .gap(px(6.0))
                        .flex_1()
                        .child(
                            ActionButton::new(SharedString::from(format!(
                                "bench-details-{run_id}"
                            )))
                            .icon(if expanded {
                                ActionIconName::Collapse
                            } else {
                                ActionIconName::Expand
                            })
                            .label(I18n::t(locale, "benchmark", "details"))
                            .ghost()
                            .palette(p)
                            .on_click(move |_, _, cx| {
                                ent_expand.update(cx, |ws, cx| {
                                    if !ws.expanded_details.remove(&run_id) {
                                        ws.expanded_details.insert(run_id);
                                    }
                                    cx.notify();
                                });
                            }),
                        )
                        .when(!is_active, |this| {
                            if confirming {
                                this.child(
                                    div()
                                        .h_flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(
                                            div().text_meta().text_color(p.danger_text).child(
                                                I18n::t(locale, "benchmark", "deleteQuestion"),
                                            ),
                                        )
                                        .child(
                                            ActionButton::new(SharedString::from(format!(
                                                "bench-confirm-del-{run_id}"
                                            )))
                                            .icon(ActionIconName::Delete)
                                            .label(I18n::t(
                                                locale,
                                                "benchmark",
                                                if self.deleting == Some(run_id) {
                                                    "deleting"
                                                } else {
                                                    "confirmDelete"
                                                },
                                            ))
                                            .danger_solid()
                                            .disabled(self.deleting.is_some())
                                            .palette(p)
                                            .on_click(move |_, _, cx| {
                                                ent_confirm
                                                    .update(cx, |ws, cx| ws.delete_run(run_id, cx));
                                            }),
                                        )
                                        .child(
                                            ActionButton::new(SharedString::from(format!(
                                                "bench-keep-{run_id}"
                                            )))
                                            .icon(ActionIconName::Cancel)
                                            .label(I18n::t(locale, "benchmark", "keepRun"))
                                            .disabled(self.deleting.is_some())
                                            .palette(p)
                                            .on_click(move |_, _, cx| {
                                                ent_keep.update(cx, |ws, cx| {
                                                    ws.confirming_delete = None;
                                                    cx.notify();
                                                });
                                            }),
                                        ),
                                )
                            } else {
                                this.child(
                                    ActionButton::new(SharedString::from(format!(
                                        "bench-del-{run_id}"
                                    )))
                                    .icon(ActionIconName::Delete)
                                    .tooltip(I18n::t(locale, "benchmark", "deleteRun"))
                                    .variant(ActionButtonVariant::Ghost)
                                    .dense_nav()
                                    .disabled(self.deleting.is_some())
                                    .palette(p)
                                    .on_click(
                                        move |_, _, cx| {
                                            ent_del.update(cx, |ws, cx| {
                                                ws.confirming_delete = Some(run_id);
                                                cx.notify();
                                            });
                                        },
                                    ),
                                )
                            }
                        }),
                );

            let mut hardware_parts: Vec<String> = Vec::new();
            if let Some(cpu) = &run.machine.cpu_model {
                hardware_parts.push(cpu.clone());
            }
            if let Some(cpus) = run.machine.logical_cpus {
                hardware_parts.push(I18n::format(
                    locale,
                    "benchmark",
                    "cpus",
                    &[("count", Param::from(i64::from(cpus)))],
                ));
            }
            if let Some(mem) = run.machine.memory_bytes {
                let gib = mem as f64 / (1024.0 * 1024.0 * 1024.0);
                hardware_parts.push(format!("{} GiB", format_decimal(locale, gib, 0, 1)));
            }
            let sha = run
                .play_reference
                .as_ref()
                .map(|r| r.sha256.as_str())
                .or_else(|| run.reference.as_ref().map(|r| r.sha256.as_str()));
            let protocol = run
                .play_reference
                .as_ref()
                .map(|r| r.protocol_id.as_str())
                .or_else(|| run.protocol.as_ref().map(|pr| pr.id.as_str()));

            rows_box = rows_box.child(
                div()
                    .v_flex()
                    .border_b_1()
                    .border_color(p.border)
                    .child(row_line)
                    .when(expanded, |this| {
                        this.child(
                            div()
                                .v_flex()
                                .gap(px(3.0))
                                .px(px(12.0))
                                .py(px(8.0))
                                .bg(p.surface_alt)
                                .text_meta()
                                .text_color(p.muted)
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(p.text)
                                        .child(format!(
                                            "{} · {} · {}",
                                            run.machine.name,
                                            run.machine.os,
                                            run.machine.architecture
                                        )),
                                )
                                .when(!hardware_parts.is_empty(), |d| {
                                    d.child(hardware_parts.join(" · "))
                                })
                                .child(format!(
                                    "{}: {}",
                                    I18n::t(locale, "benchmark", "recordedEngineVersion"),
                                    run.engine_version
                                ))
                                .when_some(run.startup_elapsed_ms, |d, ms| {
                                    let init_label = match locale {
                                        Locale::De => "Initialisierung",
                                        Locale::En => "Initialization",
                                    };
                                    d.child(format!(
                                        "{init_label}: {}",
                                        format_benchmark_ms(locale, ms)
                                    ))
                                })
                                .child(
                                    div()
                                        .mono_font()
                                        .child(format!("Path: {}", run.engine_path)),
                                )
                                .when_some(protocol, |d, proto| {
                                    d.child(div().mono_font().child(format!(
                                        "{}: {proto}",
                                        I18n::t(locale, "benchmark", "protocol")
                                    )))
                                })
                                .when_some(sha, |d, hash| {
                                    d.child(div().mono_font().child(format!("SHA-256: {hash}")))
                                }),
                        )
                    }),
            );
        }

        container.child(rows_box)
    }
}

impl EventEmitter<BenchmarkEvent> for BenchmarkWorkspace {}

impl Render for BenchmarkWorkspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = NeonTheme::current_palette(cx);
        let locale = current_locale(cx);
        let play = self.mode == BenchmarkMode::Play;
        let mode_runs: Vec<BenchmarkRun> = self
            .runs
            .iter()
            .filter(|r| r.play_reference.is_some() == play)
            .cloned()
            .collect();
        let entity = cx.entity().clone();

        let ref_section = if !play {
            self.reference.as_ref().map(|reference| {
                let open = self.methodology_open;
                let ent = entity.clone();
                div()
                    .v_flex()
                    .gap(px(6.0))
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
                            .child(div().text_body().text_color(p.text).child(format!(
                                "{} · {}",
                                reference.name,
                                I18n::format(
                                    locale,
                                    "benchmark",
                                    "referenceFacts",
                                    &[
                                        ("points", Param::from(i64::from(reference.match_length))),
                                        ("games", Param::from(i64::from(reference.total_games))),
                                        ("moves", Param::from(i64::from(reference.total_moves))),
                                    ],
                                )
                            )))
                            .child(
                                ActionButton::new("bench-methodology-toggle")
                                    .icon(if open {
                                        ActionIconName::Collapse
                                    } else {
                                        ActionIconName::Expand
                                    })
                                    .label(I18n::t(locale, "benchmark", "methodology"))
                                    .ghost()
                                    .palette(p)
                                    .on_click(move |_, _, cx| {
                                        ent.update(cx, |ws, cx| {
                                            ws.methodology_open = !ws.methodology_open;
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .when(open, |this| {
                        this.child(
                            div()
                                .v_flex()
                                .gap(px(4.0))
                                .pt(px(6.0))
                                .border_t_1()
                                .border_color(p.border)
                                .text_meta()
                                .text_color(p.muted)
                                .child(format!(
                                    "{} · {}",
                                    I18n::format(
                                        locale,
                                        "benchmark",
                                        "referenceVersion",
                                        &[("version", Param::from(i64::from(reference.version)))],
                                    ),
                                    reference.id
                                ))
                                .child(
                                    div()
                                        .mono_font()
                                        .child(format!("SHA-256 {}", reference.sha256)),
                                )
                                .child(I18n::t(locale, "benchmark", "timing")),
                        )
                    })
            })
        } else {
            None
        };

        let retry_entity = entity.clone();

        div()
            .id("benchmark-workspace")
            .test_support()
            .v_flex()
            .gap(px(14.0))
            .child(div().text_body().text_color(p.muted).child(I18n::t(
                locale,
                "benchmark",
                "description",
            )))
            .when_some(self.error_message.clone(), |this, msg| {
                this.child(
                    StatusBanner::new("benchmark-error", BannerKind::Error, msg)
                        .palette(p)
                        .on_retry(I18n::t(locale, "benchmark", "retry"), move |_, _, cx| {
                            retry_entity.update(cx, |ws, cx| ws.reload_all(cx));
                        }),
                )
            })
            .child(self.render_setup_panel(p, locale, cx))
            .when_some(self.render_active_runs(p, locale, cx), |this, sec| {
                this.child(sec)
            })
            .when_some(ref_section, |this, sec| this.child(sec))
            .when_some(
                self.render_comparison_section(&mode_runs, p, locale, cx),
                |this, sec| this.child(sec),
            )
            .child(self.render_history_table(&mode_runs, p, locale, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use neondojo::benchmarks::Machine;

    fn sample_run(
        id_seed: u128,
        status: &str,
        plies: i32,
        avg_ms: Option<f64>,
        finished_sec: i64,
    ) -> BenchmarkRun {
        BenchmarkRun {
            engine_snapshot: None,
            id: Uuid::from_u128(id_seed),
            status: status.to_string(),
            created_at: Utc.timestamp_opt(1_700_000_000 + finished_sec, 0).unwrap(),
            started_at: Some(Utc.timestamp_opt(1_700_000_000 + finished_sec, 0).unwrap()),
            finished_at: Some(Utc.timestamp_opt(1_700_000_010 + finished_sec, 0).unwrap()),
            machine: Machine {
                id: Uuid::from_u128(1),
                name: "M4 Pro".into(),
                os: "macOS".into(),
                architecture: "aarch64".into(),
                cpu_model: Some("Apple M4 Pro".into()),
                logical_cpus: Some(14),
                memory_bytes: Some(64 * 1024 * 1024 * 1024),
            },
            engine_id: "gnubg".into(),
            engine_name: "GNUbg".into(),
            engine_version: "GNU Backgammon 1.08.003".into(),
            engine_path: "/usr/local/bin/gnubg".into(),
            plies,
            reference: None,
            protocol: None,
            play_reference: None,
            startup_elapsed_ms: None,
            total_elapsed_ms: avg_ms.map(|v| v * 460.0),
            average_move_ms: avg_ms,
            average_turn_ms: None,
            completed_games: 10,
            analyzed_moves: 460,
            completed_turns: 0,
            error: None,
        }
    }

    #[test]
    fn comparison_requires_at_least_three_matching_completed_runs_and_computes_median_and_range() {
        let two = vec![
            sample_run(1, "completed", 3, Some(10.0), 10),
            sample_run(2, "completed", 3, Some(12.0), 20),
        ];
        assert!(benchmark_comparisons(&two).is_empty());

        // 4 runs: oldest (100ms) is ignored because only the 3 most recently finished count.
        let four = vec![
            sample_run(1, "completed", 3, Some(100.0), 10),
            sample_run(2, "completed", 3, Some(18.0), 20),
            sample_run(3, "completed", 3, Some(12.0), 30),
            sample_run(4, "completed", 3, Some(15.0), 40),
            sample_run(5, "failed", 3, Some(1.0), 50),
        ];
        let cmp = benchmark_comparisons(&four);
        assert_eq!(cmp.len(), 1);
        assert_eq!(cmp[0].count, 3);
        assert!((cmp[0].minimum - 12.0).abs() < 1e-9);
        assert!((cmp[0].median - 15.0).abs() < 1e-9);
        assert!((cmp[0].maximum - 18.0).abs() < 1e-9);
        assert_eq!(cmp[0].run.id, Uuid::from_u128(4));
    }

    #[test]
    fn sort_benchmark_runs_keeps_unmeasured_last_and_orders_status_lifecycle() {
        let runs = vec![
            sample_run(1, "completed", 3, Some(25.0), 10),
            sample_run(2, "running", 3, None, 20),
            sample_run(3, "completed", 3, Some(10.0), 30),
            sample_run(4, "failed", 3, None, 40),
        ];

        let asc = sort_benchmark_runs(
            &runs,
            BenchmarkHistoryOrder {
                sort: BenchmarkHistorySort::Average,
                direction: SortDirection::Asc,
            },
            false,
            Locale::En,
        );
        assert_eq!(asc[0].id, Uuid::from_u128(3));
        assert_eq!(asc[1].id, Uuid::from_u128(1));
        assert_eq!(asc[2].id, Uuid::from_u128(2));
        assert_eq!(asc[3].id, Uuid::from_u128(4));

        let desc = sort_benchmark_runs(
            &runs,
            BenchmarkHistoryOrder {
                sort: BenchmarkHistorySort::Average,
                direction: SortDirection::Desc,
            },
            false,
            Locale::En,
        );
        assert_eq!(desc[0].id, Uuid::from_u128(1));
        assert_eq!(desc[1].id, Uuid::from_u128(3));
        assert_eq!(desc[2].id, Uuid::from_u128(2));

        let by_status = sort_benchmark_runs(
            &runs,
            BenchmarkHistoryOrder {
                sort: BenchmarkHistorySort::Status,
                direction: SortDirection::Asc,
            },
            false,
            Locale::En,
        );
        assert_eq!(by_status[0].status, "running");
        assert_eq!(by_status[1].status, "completed");
        assert_eq!(by_status[3].status, "failed");
    }
}
