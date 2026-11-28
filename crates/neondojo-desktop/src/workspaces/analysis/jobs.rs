//! Persistent `MatchAnalysisJobEntity` controller owned inside `AnalysisWorkspace`.
//!
//! Because `AnalysisWorkspace` keeps this entity alive independently of visible
//! tabs or active workspace selection, switching away to Library/Play/Study NEVER
//! cancels or drops running match/game/position analysis tracking, live
//! `watch_job_events` subscriptions, or the 1-second monotonic ETA estimator.

use super::state::{MonotonicRemainingTimer, TimerPhase, format_eta_ms, run_facade};
use crate::app_context::DesktopAppContext;
use crate::tokio_bridge::TokioBridge;
use futures_util::StreamExt;
use gpui_kit::{App, Context, EventEmitter, Task, Window};
use neondojo::{
    api::{JobRequest, MatchAnalysisEngine, MatchAnalysisRequest, PositionJobKind},
    benchmarks::estimate::{AnalysisTimeEstimate, EstimateQuery},
    domain_error::DomainError,
    model::Job,
    settings::EngineInfo,
};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

/// Events emitted by [`MatchAnalysisJobEntity`] to [`super::AnalysisWorkspace`].
#[derive(Debug, Clone)]
pub enum AnalysisJobEvent {
    /// A match or game job transitioned (progress, completed, failed, cancelled).
    MatchJobUpdated { match_id: Uuid, job: Job },
    /// A single-position checker analysis job transitioned.
    PositionAnalysisJobUpdated { position_id: Uuid, job: Job },
    /// A dice-equity job transitioned.
    DiceJobUpdated {
        position_id: Uuid,
        engine: MatchAnalysisEngine,
        job: Job,
    },
    /// A job submission or cancellation failed.
    Error(String),
}

/// Analysis launch options shown inside the header Options popover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisLaunchOptions {
    pub engine: MatchAnalysisEngine,
    pub plies: i32,
    pub scope_current_game: bool,
}

impl Default for AnalysisLaunchOptions {
    fn default() -> Self {
        Self {
            engine: MatchAnalysisEngine::Gnubg,
            plies: 3,
            scope_current_game: false,
        }
    }
}

/// Persistent job tracker surviving workspace and tab switches.
pub struct MatchAnalysisJobEntity {
    pub engines: Vec<EngineInfo>,
    pub options: AnalysisLaunchOptions,
    pub estimate: Option<AnalysisTimeEstimate>,
    pub estimate_loading: bool,
    pub active_match_jobs: HashMap<Uuid, Job>,
    pub active_position_jobs: HashMap<Uuid, Job>,
    pub active_dice_jobs: HashMap<Uuid, (MatchAnalysisEngine, Job)>,
    pub remaining: HashMap<Uuid, MonotonicRemainingTimer>,
    pub busy_submitting: bool,
    pub cancelling_jobs: HashMap<Uuid, bool>,
    pub last_error: Option<String>,
    watch_tasks: HashMap<Uuid, Task<()>>,
    countdown_task: Option<Task<()>>,
}

impl EventEmitter<AnalysisJobEvent> for MatchAnalysisJobEntity {}

impl MatchAnalysisJobEntity {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut entity = Self {
            engines: Vec::new(),
            options: AnalysisLaunchOptions::default(),
            estimate: None,
            estimate_loading: false,
            active_match_jobs: HashMap::new(),
            active_position_jobs: HashMap::new(),
            active_dice_jobs: HashMap::new(),
            remaining: HashMap::new(),
            busy_submitting: false,
            cancelling_jobs: HashMap::new(),
            last_error: None,
            watch_tasks: HashMap::new(),
            countdown_task: None,
        };
        entity.start_countdown_ticker(cx);
        entity.refresh_engines(cx);
        entity
    }

    fn start_countdown_ticker(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let keep_going = this
                    .update(cx, |state, cx| {
                        let mut changed = false;
                        for timer in state.remaining.values_mut() {
                            if matches!(timer.state, Some(TimerPhase::Estimated)) {
                                timer.tick_one_second();
                                changed = true;
                            }
                        }
                        if changed {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !keep_going {
                    break;
                }
            }
        });
        self.countdown_task = Some(task);
    }

    /// Loads available engines and default analysis settings from the backend.
    pub fn refresh_engines(&mut self, cx: &mut Context<Self>) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let facade_clone = facade.clone();
        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move {
                let engines = facade_clone.list_engines().await?;
                let settings = facade_clone.get_settings().await.ok();
                Ok((engines, settings))
            })
            .await
        });

        cx.spawn(async move |this, cx| {
            let Ok(Ok((engines, settings))) = read.await else {
                return;
            };
            let _ = this.update(cx, |state, cx| {
                state.engines = engines;
                if let Some(s) = settings {
                    state.options.engine = if s.analysis_engine == "bgsage" {
                        MatchAnalysisEngine::Bgsage
                    } else {
                        MatchAnalysisEngine::Gnubg
                    };
                    state.options.plies = s.analysis_plies;
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Returns verified plies for `engine` (respecting unified ply convention:
    /// GNUbg 1..=5, NeonGammon 1..=4).
    pub fn allowed_plies(&self, engine: MatchAnalysisEngine) -> Vec<i32> {
        let target_id = match engine {
            MatchAnalysisEngine::Gnubg => "gnubg",
            MatchAnalysisEngine::Bgsage => "bgsage",
        };
        if let Some(info) = self.engines.iter().find(|e| e.id == target_id)
            && !info.plies.is_empty()
        {
            return info.plies.clone();
        }
        match engine {
            MatchAnalysisEngine::Gnubg => vec![1, 2, 3, 4, 5],
            MatchAnalysisEngine::Bgsage => vec![1, 2, 3, 4],
        }
    }

    pub fn engine_available(&self, engine: MatchAnalysisEngine) -> bool {
        let target_id = match engine {
            MatchAnalysisEngine::Gnubg => "gnubg",
            MatchAnalysisEngine::Bgsage => "bgsage",
        };
        self.engines
            .iter()
            .find(|e| e.id == target_id)
            .map(|e| e.available)
            .unwrap_or(true)
    }

    pub fn set_options(
        &mut self,
        engine: MatchAnalysisEngine,
        plies: i32,
        scope_current_game: bool,
        match_id: Option<Uuid>,
        current_game_no: Option<i32>,
        cx: &mut Context<Self>,
    ) {
        let allowed = self.allowed_plies(engine);
        let clamped_plies = if allowed.contains(&plies) {
            plies
        } else {
            *allowed.last().unwrap_or(&3)
        };
        self.options = AnalysisLaunchOptions {
            engine,
            plies: clamped_plies,
            scope_current_game,
        };
        cx.notify();
        if let Some(mid) = match_id {
            self.refresh_estimate(
                mid,
                if scope_current_game {
                    current_game_no
                } else {
                    None
                },
                cx,
            );
        }
    }

    /// Loads `AnalysisTimeEstimate` for the selected engine, ply depth, and scope.
    pub fn refresh_estimate(
        &mut self,
        match_id: Uuid,
        game_no: Option<i32>,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.estimate_loading = true;
        cx.notify();

        let engine_id = match self.options.engine {
            MatchAnalysisEngine::Gnubg => "gnubg".to_string(),
            MatchAnalysisEngine::Bgsage => "bgsage".to_string(),
        };
        let plies = self.options.plies;

        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move {
                facade
                    .estimate_analysis_time(
                        match_id,
                        EstimateQuery {
                            plies: Some(plies),
                            game_no,
                            engine_id: Some(engine_id),
                        },
                    )
                    .await
            })
            .await
        });

        cx.spawn(async move |this, cx| {
            let res = read.await;
            let _ = this.update(cx, |state, cx| {
                state.estimate_loading = false;
                if let Ok(Ok(est)) = res {
                    state.estimate = Some(est);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Checks for any already-running match or game job for `match_id` and attaches watchers.
    pub fn attach_existing_match_jobs(&mut self, match_id: Uuid, cx: &mut Context<Self>) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move { facade.list_match_analysis_jobs(match_id, None).await })
                .await
        });

        cx.spawn(async move |this, cx| {
            let Ok(Ok(runs)) = read.await else {
                return;
            };
            let _ = this.update(cx, |state, cx| {
                for run in runs {
                    if matches!(run.job.status.as_str(), "queued" | "running") {
                        let job = run.job;
                        state.track_match_job(match_id, job, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Starts a match or single-game analysis job via `facade.create_match_analysis`.
    pub fn start_match_analysis(
        &mut self,
        match_id: Uuid,
        game_no: Option<i32>,
        engine_override: Option<MatchAnalysisEngine>,
        plies_override: Option<i32>,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            self.last_error = Some("Backend not connected".to_string());
            cx.notify();
            return;
        };
        self.busy_submitting = true;
        self.last_error = None;
        cx.notify();

        let request = MatchAnalysisRequest {
            game_no,
            engine_id: Some(engine_override.unwrap_or(self.options.engine)),
            plies: Some(plies_override.unwrap_or(self.options.plies)),
        };

        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move { facade.create_match_analysis(match_id, request).await })
                .await
        });

        cx.spawn(async move |this, cx| {
            let res = write.await;
            let _ = this.update(cx, |state, cx| {
                state.busy_submitting = false;
                match res {
                    Ok(Ok(job)) => {
                        state.track_match_job(match_id, job.clone(), cx);
                        cx.emit(AnalysisJobEvent::MatchJobUpdated { match_id, job });
                    }
                    Ok(Err(err)) => {
                        let msg = err.to_string();
                        state.last_error = Some(msg.clone());
                        cx.emit(AnalysisJobEvent::Error(msg));
                    }
                    Err(err) => {
                        let msg = format!("Job submission interrupted: {err}");
                        state.last_error = Some(msg.clone());
                        cx.emit(AnalysisJobEvent::Error(msg));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Submits a single-position checker or dice calculation job (`create_position_job`).
    pub fn start_position_job(
        &mut self,
        position_id: Uuid,
        kind: PositionJobKind,
        engine: MatchAnalysisEngine,
        plies: Option<i32>,
        cx: &mut Context<Self>,
    ) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            let msg = "Backend not connected".to_string();
            self.last_error = Some(msg.clone());
            cx.emit(AnalysisJobEvent::Error(msg));
            cx.notify();
            return;
        };

        let request = JobRequest {
            position_id,
            kind,
            engine_id: Some(engine),
            plies,
        };

        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move { facade.create_position_job(request).await }).await
        });

        cx.spawn(async move |this, cx| {
            let res = write.await;
            let _ = this.update(cx, |state, cx| {
                match res {
                    Ok(Ok(job)) => match kind {
                        PositionJobKind::Analysis => {
                            state.track_position_job(position_id, job.clone(), cx);
                            cx.emit(AnalysisJobEvent::PositionAnalysisJobUpdated {
                                position_id,
                                job,
                            });
                        }
                        PositionJobKind::Dice => {
                            state.track_dice_job(position_id, engine, job.clone(), cx);
                            cx.emit(AnalysisJobEvent::DiceJobUpdated {
                                position_id,
                                engine,
                                job,
                            });
                        }
                    },
                    Ok(Err(err)) => {
                        let msg = err.to_string();
                        state.last_error = Some(msg.clone());
                        cx.emit(AnalysisJobEvent::Error(msg));
                    }
                    Err(err) => {
                        let msg = format!("Position job interrupted: {err}");
                        state.last_error = Some(msg.clone());
                        cx.emit(AnalysisJobEvent::Error(msg));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Cancels a running/queued job via `facade.cancel_job`.
    pub fn cancel_job(&mut self, job_id: Uuid, cx: &mut Context<Self>) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        self.cancelling_jobs.insert(job_id, true);
        cx.notify();

        let write = TokioBridge::spawn_write(cx, async move {
            run_facade(move || async move { facade.cancel_job(job_id).await }).await
        });

        cx.spawn(async move |this, cx| {
            let res = write.await;
            let _ = this.update(cx, |state, cx| {
                state.cancelling_jobs.remove(&job_id);
                match res {
                    Ok(Ok(job)) => {
                        state.apply_job_snapshot(job, cx);
                    }
                    Ok(Err(err)) => {
                        let msg = err.to_string();
                        state.last_error = Some(msg.clone());
                        cx.emit(AnalysisJobEvent::Error(msg));
                    }
                    Err(err) => {
                        let msg = format!("Cancel interrupted: {err}");
                        state.last_error = Some(msg.clone());
                        cx.emit(AnalysisJobEvent::Error(msg));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn active_jobs_for_match(&self, match_id: Uuid) -> Vec<&Job> {
        let mut jobs: Vec<&Job> = self
            .active_match_jobs
            .values()
            .filter(|j| {
                j.match_uuid == Some(match_id) && matches!(j.status.as_str(), "queued" | "running")
            })
            .collect();
        jobs.sort_by_key(|j| j.created_at);
        jobs
    }

    pub fn remaining_label(&self, job_id: Uuid) -> Option<String> {
        let timer = self.remaining.get(&job_id)?;
        match timer.state {
            Some(TimerPhase::Queued) => Some("Queued".to_string()),
            Some(TimerPhase::Waiting) => Some("Waiting for worker".to_string()),
            Some(TimerPhase::Finishing) => Some("Finishing…".to_string()),
            Some(TimerPhase::Reestimating) => Some("Re-estimating…".to_string()),
            Some(TimerPhase::Estimated) => timer
                .display_remaining_ms
                .map(|ms| format!("~{} left", format_eta_ms(ms))),
            _ => None,
        }
    }

    /// Begins tracking a match/game job and subscribes to `watch_job_events`.
    pub fn track_match_job(&mut self, _match_id: Uuid, job: Job, cx: &mut Context<Self>) {
        let job_id = job.id;
        let active = matches!(job.status.as_str(), "queued" | "running");
        self.active_match_jobs.insert(job_id, job);
        if active {
            self.ensure_watcher(job_id, JobWatchTarget::Match, cx);
            self.poll_remaining_time(job_id, cx);
        }
    }

    pub fn track_position_job(&mut self, _position_id: Uuid, job: Job, cx: &mut Context<Self>) {
        let job_id = job.id;
        let active = matches!(job.status.as_str(), "queued" | "running");
        self.active_position_jobs.insert(job_id, job);
        if active {
            self.ensure_watcher(job_id, JobWatchTarget::PositionAnalysis, cx);
        }
    }

    pub fn track_dice_job(
        &mut self,
        _position_id: Uuid,
        engine: MatchAnalysisEngine,
        job: Job,
        cx: &mut Context<Self>,
    ) {
        let job_id = job.id;
        let active = matches!(job.status.as_str(), "queued" | "running");
        self.active_dice_jobs.insert(job_id, (engine, job));
        if active {
            self.ensure_watcher(job_id, JobWatchTarget::Dice, cx);
        }
    }

    fn apply_job_snapshot(&mut self, job: Job, cx: &mut Context<Self>) {
        let job_id = job.id;
        let terminal = matches!(job.status.as_str(), "completed" | "failed");
        if terminal {
            self.watch_tasks.remove(&job_id);
            self.remaining.remove(&job_id);
        }
        if let Some(entry) = self.active_match_jobs.get_mut(&job_id) {
            *entry = job.clone();
            if let Some(mid) = job.match_uuid {
                cx.emit(AnalysisJobEvent::MatchJobUpdated {
                    match_id: mid,
                    job: job.clone(),
                });
            }
        }
        if let Some(entry) = self.active_position_jobs.get_mut(&job_id) {
            *entry = job.clone();
            if let Some(pid) = job.position_uuid {
                cx.emit(AnalysisJobEvent::PositionAnalysisJobUpdated {
                    position_id: pid,
                    job: job.clone(),
                });
            }
        }
        if let Some((engine, entry)) = self.active_dice_jobs.get_mut(&job_id) {
            let eng = *engine;
            *entry = job.clone();
            if let Some(pid) = job.position_uuid {
                cx.emit(AnalysisJobEvent::DiceJobUpdated {
                    position_id: pid,
                    engine: eng,
                    job,
                });
            }
        }
    }

    fn poll_remaining_time(&self, job_id: Uuid, cx: &mut Context<Self>) {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };
        let read = TokioBridge::spawn_read(cx, async move {
            run_facade(move || async move { facade.job_remaining_time(job_id).await }).await
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(sample)) = read.await else {
                return;
            };
            let _ = this.update(cx, |state, cx| {
                state
                    .remaining
                    .entry(job_id)
                    .or_default()
                    .sync_from_server(&sample);
                cx.notify();
            });
        })
        .detach();
    }

    fn ensure_watcher(&mut self, job_id: Uuid, target: JobWatchTarget, cx: &mut Context<Self>) {
        if self.watch_tasks.contains_key(&job_id) {
            return;
        }
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return;
        };

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
        let tokio_handle = TokioBridge::handle(cx);
        let watcher_facade = facade.clone();
        let stream_handle = tokio_handle.spawn(async move {
            let stream_res =
                run_facade(move || async move { watcher_facade.watch_job_events(job_id, 0).await })
                    .await;
            let Ok(stream) = stream_res else {
                return;
            };
            futures_util::pin_mut!(stream);
            while let Some(_event) = stream.next().await {
                let f = facade.clone();
                if let Ok(job) = run_facade(move || async move { f.get_job(job_id).await }).await {
                    let terminal = matches!(job.status.as_str(), "completed" | "failed");
                    let _ = tx.send(job);
                    if terminal {
                        break;
                    }
                }
            }
        });

        let gpui_task = cx.spawn(async move |this, cx| {
            while let Some(job) = rx.recv().await {
                let terminal = matches!(job.status.as_str(), "completed" | "failed");
                let ok = this
                    .update(cx, |state, cx| {
                        state.apply_job_snapshot(job.clone(), cx);
                        if !terminal && matches!(target, JobWatchTarget::Match) {
                            state.poll_remaining_time(job_id, cx);
                        }
                        cx.notify();
                    })
                    .is_ok();
                if !ok || terminal {
                    break;
                }
            }
            stream_handle.abort();
        });

        self.watch_tasks.insert(job_id, gpui_task);
    }

    /// Helper to open the Recalculate Position or Recalculate Dice modal dialog.
    pub fn open_recalculate_dialog(
        &self,
        position_id: Uuid,
        kind: PositionJobKind,
        initial_engine: MatchAnalysisEngine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::theme::{NeonTheme, NeonTypographyExt};
        use crate::ui::action_button::ActionButton;
        use crate::ui::modal::{ModalConfig, ModalSize, open_modal_dialog};
        use gpui_kit::{
            AppContext, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
            Styled, TestSupportExt, base::StyledExt, component::WindowExt, div, px,
        };

        let entity = cx.entity().clone();
        let initial_plies = self.options.plies;
        let selection = cx.new(|_| RecalcDialogState {
            engine: initial_engine,
            plies: initial_plies,
        });

        let title = match kind {
            PositionJobKind::Analysis => "Recalculate position",
            PositionJobKind::Dice => "Recalculate dice equity",
        };
        let cfg = ModalConfig::new(title).size(ModalSize::Compact);

        let body_sel = selection.clone();
        let body_entity = entity.clone();
        let footer_sel = selection.clone();
        let footer_entity = entity.clone();

        open_modal_dialog(
            window,
            cx,
            cfg,
            move |_window: &mut Window, cx: &mut App| {
                let p = NeonTheme::current_palette(cx);
                let cur = body_sel.read(cx).clone();
                let allowed = body_entity.read(cx).allowed_plies(cur.engine);

                div()
                    .id("recalc-dialog-body")
                    .test_support()
                    .v_flex()
                    .gap(px(12.0))
                    .child(
                        div()
                            .v_flex()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.muted)
                                    .child("Engine"),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .gap(px(8.0))
                                    .child({
                                        let sel = body_sel.clone();
                                        let ent = body_entity.clone();
                                        ActionButton::new("recalc-engine-gnubg")
                                            .label("GNUbg")
                                            .active(cur.engine == MatchAnalysisEngine::Gnubg)
                                            .palette(p)
                                            .on_click(move |_, _, cx| {
                                                let max_p = *ent
                                                    .read(cx)
                                                    .allowed_plies(MatchAnalysisEngine::Gnubg)
                                                    .last()
                                                    .unwrap_or(&3);
                                                sel.update(cx, |s, cx| {
                                                    s.engine = MatchAnalysisEngine::Gnubg;
                                                    s.plies = s.plies.min(max_p);
                                                    cx.notify();
                                                });
                                            })
                                    })
                                    .child({
                                        let sel = body_sel.clone();
                                        let ent = body_entity.clone();
                                        ActionButton::new("recalc-engine-bgsage")
                                            .label("NeonGammon")
                                            .active(cur.engine == MatchAnalysisEngine::Bgsage)
                                            .palette(p)
                                            .on_click(move |_, _, cx| {
                                                let max_p = *ent
                                                    .read(cx)
                                                    .allowed_plies(MatchAnalysisEngine::Bgsage)
                                                    .last()
                                                    .unwrap_or(&3);
                                                sel.update(cx, |s, cx| {
                                                    s.engine = MatchAnalysisEngine::Bgsage;
                                                    s.plies = s.plies.min(max_p);
                                                    cx.notify();
                                                });
                                            })
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .v_flex()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_label()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.muted)
                                    .child("Depth (XG / NeonGammon ply)"),
                            )
                            .child(
                                div()
                                    .h_flex()
                                    .gap(px(6.0))
                                    .children(allowed.into_iter().map(|ply| {
                                        let sel = body_sel.clone();
                                        ActionButton::new(SharedString::from(format!(
                                            "recalc-ply-{ply}"
                                        )))
                                        .label(SharedString::from(format!("{ply}-ply")))
                                        .active(cur.plies == ply)
                                        .palette(p)
                                        .on_click(
                                            move |_, _, cx| {
                                                sel.update(cx, |s, cx| {
                                                    s.plies = ply;
                                                    cx.notify();
                                                });
                                            },
                                        )
                                    })),
                            ),
                    )
                    .into_any_element()
            },
            Some(move |_window: &mut Window, cx: &mut App| {
                let p = NeonTheme::current_palette(cx);
                let sel = footer_sel.clone();
                let ent = footer_entity.clone();
                div()
                    .h_flex()
                    .gap(px(8.0))
                    .justify_end()
                    .child(
                        ActionButton::new("recalc-cancel")
                            .label("Cancel")
                            .palette(p)
                            .on_click(|_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    )
                    .child(
                        ActionButton::new("recalc-submit")
                            .label("Calculate")
                            .primary()
                            .palette(p)
                            .on_click(move |_, window, cx| {
                                let chosen = sel.read(cx).clone();
                                ent.update(cx, |jobs, cx| {
                                    jobs.start_position_job(
                                        position_id,
                                        kind,
                                        chosen.engine,
                                        Some(chosen.plies),
                                        cx,
                                    );
                                });
                                window.close_dialog(cx);
                            }),
                    )
                    .into_any_element()
            }),
            None,
        );
    }
}

#[derive(Debug, Clone)]
struct RecalcDialogState {
    engine: MatchAnalysisEngine,
    plies: i32,
}

#[derive(Debug, Clone, Copy)]
enum JobWatchTarget {
    Match,
    PositionAnalysis,
    Dice,
}

impl Drop for MatchAnalysisJobEntity {
    fn drop(&mut self) {
        self.watch_tasks.clear();
        self.countdown_task = None;
    }
}

/// Formats a domain error into a compact user-facing message.
pub fn format_job_error(err: &DomainError) -> String {
    err.to_string()
}
