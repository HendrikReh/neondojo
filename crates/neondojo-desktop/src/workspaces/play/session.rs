//! Single-flight Play session command queue (`PlaySessionEntity`).
//!
//! Enforces strict concurrency invariants mirroring `usePlaySession.ts`:
//! - **Abortable `InitialLoad`** via `TokioBridge::spawn_read` (`ReadTask`):
//!   an explicit `open_session` or retry replaces/aborts an in-flight read.
//! - **Non-abortable `Mutation`** via `TokioBridge::spawn_write`:
//!   never drops or cancels an in-flight move/cube/resign/pause/mode submission,
//!   and rejects any concurrent command while a mutation is locked.
//! - Atomically persists the active session ID in `LocalUiState::play_session_id`.
//! - Emits `PlaySessionEvent::SwitchedToCompetition { match_id }` when changing
//!   live mode to `PlayMode::Competition`.

use crate::app_context::DesktopAppContext;
use crate::i18n::{current_locale, format_domain_error};
use crate::local_state::LocalUiState;
use crate::tokio_bridge::{ReadTask, TokioBridge};
use crate::workspaces::play::interruption::{PlayActionKind, is_resignation_declined};
use gpui_kit::{App, Context, EventEmitter, Task, Window};
use neondojo::domain_error::{DomainError, ErrorKind};
use neondojo::facade::BackendFacade;
use neondojo::play::{
    CreatePlaySession, FeedbackOperation, PlayActionRequest, PlayFeedbackRequest, PlayMode,
    PlayModeRequest, PlayReviewRequest, PlaySession,
};
use std::future::Future;
use std::path::PathBuf;
use uuid::Uuid;

/// Wraps any `BackendFacade` async operation inside `spawn_blocking + block_on`
/// to avoid cross-crate SQLx HRTB `Send` issues when spawning onto Tokio.
pub(crate) async fn run_facade<T, F, Fut>(facade: BackendFacade, f: F) -> Result<T, DomainError>
where
    T: Send + 'static,
    F: FnOnce(BackendFacade) -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, DomainError>> + 'static,
{
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || handle.block_on(f(facade)))
        .await
        .map_err(|err| {
            DomainError::new(ErrorKind::Internal, format!("Backend task failed: {err}"))
        })?
}

/// Summary of an accepted session checkpoint.
#[derive(Debug, Clone)]
pub struct AcceptedSession {
    pub session: PlaySession,
    pub previous: Option<PlaySession>,
    pub action: Option<PlayActionKind>,
    pub same_session: bool,
    pub same_game: bool,
    pub preserve_steps: bool,
    /// True when accepted from a load/restore rather than a live action (suppresses sounds).
    pub from_load: bool,
}

/// Structured error surfaced by [`PlaySessionEntity`].
#[derive(Debug, Clone)]
pub struct PlaySessionError {
    pub kind: ErrorKind,
    pub message: String,
    pub resignation_declined: bool,
}

impl PlaySessionError {
    pub fn from_domain(locale: neondojo_i18n::Locale, err: &DomainError) -> Self {
        Self {
            kind: err.kind,
            message: format_domain_error(locale, err),
            resignation_declined: is_resignation_declined(err),
        }
    }

    pub fn simple(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            resignation_declined: false,
        }
    }
}

/// Events emitted by [`PlaySessionEntity`] to `PlayWorkspace`.
#[derive(Debug, Clone)]
pub enum PlaySessionEvent {
    /// Emitted immediately before replacing the session with a load.
    LoadingStarted,
    /// Emitted when a session checkpoint is accepted.
    Accepted(Box<AcceptedSession>),
    /// Emitted when a restored session was deleted (`404`) and setup should open.
    StoredSessionMissing,
    /// Emitted when live mode switches to `Competition` so Analysis invalidates cache.
    SwitchedToCompetition { match_id: Uuid },
    /// Emitted when a match in the library may have been created or updated.
    LibraryMutated,
    /// Emitted when a request fails.
    Error(PlaySessionError),
}

/// Single-flight execution slot distinguishing abortable reads from
/// non-abortable mutations.
enum InFlight {
    None,
    InitialLoad {
        _task: ReadTask<()>,
        _observer: Task<()>,
    },
    Mutation {
        _observer: Task<()>,
    },
    /// Synchronous test lock used by unit tests to verify single-flight rejection.
    #[cfg(test)]
    TestMutationLock,
}

impl InFlight {
    fn is_busy(&self) -> bool {
        !matches!(self, Self::None)
    }

    fn is_mutation(&self) -> bool {
        match self {
            Self::Mutation { .. } => true,
            #[cfg(test)]
            Self::TestMutationLock => true,
            Self::None | Self::InitialLoad { .. } => false,
        }
    }

    fn is_initial_load(&self) -> bool {
        matches!(self, Self::InitialLoad { .. })
    }
}

/// Single-flight session state and async command coordinator.
pub struct PlaySessionEntity {
    session: Option<PlaySession>,
    in_flight: InFlight,
    generation: u64,
    error: Option<PlaySessionError>,
    retry_session_id: Option<Uuid>,
    persist_path: Option<PathBuf>,
}

impl EventEmitter<PlaySessionEvent> for PlaySessionEntity {}

impl PlaySessionEntity {
    pub fn new(persist_path: Option<PathBuf>) -> Self {
        Self {
            session: None,
            in_flight: InFlight::None,
            generation: 0,
            error: None,
            retry_session_id: None,
            persist_path,
        }
    }

    pub fn session(&self) -> Option<&PlaySession> {
        self.session.as_ref()
    }

    pub fn is_busy(&self) -> bool {
        self.in_flight.is_busy()
    }

    pub fn is_locked(&self) -> bool {
        self.in_flight.is_busy()
    }

    pub fn is_mutation_in_flight(&self) -> bool {
        self.in_flight.is_mutation()
    }

    pub fn is_initial_load_in_flight(&self) -> bool {
        self.in_flight.is_initial_load()
    }

    pub fn error(&self) -> Option<&PlaySessionError> {
        self.error.as_ref()
    }

    pub fn clear_error(&mut self, cx: &mut Context<Self>) {
        if self.error.take().is_some() {
            cx.notify();
        }
    }

    pub fn retry_session_id(&self) -> Option<Uuid> {
        self.retry_session_id
    }

    /// Injects a session directly (used in headless/unit tests without PostgreSQL).
    pub fn set_session_for_test(
        &mut self,
        session: PlaySession,
        preserve_steps: bool,
        cx: &mut Context<Self>,
    ) {
        self.accept_session(session, None, preserve_steps, true, cx);
    }

    /// Acquires a synthetic mutation lock for unit-testing single-flight rejection.
    #[cfg(test)]
    pub fn acquire_test_mutation_lock(&mut self) -> bool {
        if self.in_flight.is_busy() {
            return false;
        }
        self.in_flight = InFlight::TestMutationLock;
        true
    }

    /// Releases the synthetic test mutation lock.
    #[cfg(test)]
    pub fn release_test_mutation_lock(&mut self) {
        if matches!(self.in_flight, InFlight::TestMutationLock) {
            self.in_flight = InFlight::None;
        }
    }

    /// Checks whether a new mutation command can acquire the single-flight lock.
    pub fn can_start_mutation(&self) -> bool {
        !self.in_flight.is_busy()
    }

    /// Checks whether a load command can start (allowed when idle OR aborting an
    /// existing `InitialLoad`, NEVER allowed while a `Mutation` is in flight).
    pub fn can_start_load(&self) -> bool {
        !self.in_flight.is_mutation()
    }

    /// Loads a session by ID via abortable `TokioBridge::spawn_read` (`ReadTask`).
    ///
    /// If `resume == true` and the loaded session is paused, transitions to a
    /// non-abortable `TokioBridge::spawn_write` to commit `resume_play_session`.
    pub fn load_session(
        &mut self,
        session_id: Uuid,
        resume: bool,
        restoring: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.can_start_load() {
            return false;
        }
        let Some(_facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return false;
        };

        // Dropping any prior InFlight::InitialLoad immediately aborts its ReadTask!
        self.in_flight = InFlight::None;
        self.error = None;
        self.retry_session_id = Some(session_id);
        self.generation = self.generation.wrapping_add(1);
        cx.emit(PlaySessionEvent::LoadingStarted);

        self.spawn_abortable_load(session_id, resume, restoring, window, cx)
    }

    fn spawn_abortable_load(
        &mut self,
        session_id: Uuid,
        resume: bool,
        restoring: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return false;
        };
        let token = self.generation;
        let facade_read = facade.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<Result<PlaySession, DomainError>>();
        let read_task: ReadTask<()> = TokioBridge::spawn_read(cx, async move {
            let res = run_facade(facade_read, move |f| async move {
                f.get_play_session(session_id).await
            })
            .await;
            let _ = tx.send(res);
        });

        let observer = cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = rx.await else {
                // Aborted because a newer load replaced this ReadTask.
                return;
            };
            match result {
                Ok(loaded) if resume && loaded.paused => {
                    // Transition from abortable InitialLoad to non-abortable Mutation!
                    let version = loaded.version;
                    let resume_started = this
                        .update_in(cx, |entity, window, cx| {
                            if entity.generation != token {
                                return false;
                            }
                            entity.start_resume_after_load(
                                token,
                                loaded.clone(),
                                session_id,
                                version,
                                facade,
                                window,
                                cx,
                            );
                            true
                        })
                        .unwrap_or(false);
                    let _ = resume_started;
                }
                Ok(loaded) => {
                    let _ = this.update_in(cx, |entity, _window, cx| {
                        if entity.generation != token {
                            return;
                        }
                        entity.in_flight = InFlight::None;
                        entity.accept_session(loaded, None, false, true, cx);
                    });
                }
                Err(err) => {
                    let _ = this.update_in(cx, |entity, _window, cx| {
                        if entity.generation != token {
                            return;
                        }
                        entity.in_flight = InFlight::None;
                        if restoring && err.kind == ErrorKind::NotFound {
                            entity.forget_stored_session(session_id, cx);
                            entity.retry_session_id = None;
                            entity.error = None;
                            cx.emit(PlaySessionEvent::StoredSessionMissing);
                            cx.notify();
                        } else {
                            let parsed = PlaySessionError::from_domain(current_locale(cx), &err);
                            entity.error = Some(parsed.clone());
                            cx.emit(PlaySessionEvent::Error(parsed));
                            cx.notify();
                        }
                    });
                }
            }
        });

        self.in_flight = InFlight::InitialLoad {
            _task: read_task,
            _observer: observer,
        };
        cx.notify();
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn start_resume_after_load(
        &mut self,
        token: u64,
        fallback_session: PlaySession,
        session_id: Uuid,
        version: i64,
        facade: BackendFacade,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Once resume mutation starts, use non-abortable spawn_write.
        let write_task = TokioBridge::spawn_write(
            cx,
            run_facade(facade, move |f| async move {
                f.resume_play_session(session_id, version).await
            }),
        );
        let observer = cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = write_task.await else {
                return;
            };
            let _ = this.update_in(cx, |entity, _window, cx| {
                if entity.generation != token {
                    return;
                }
                entity.in_flight = InFlight::None;
                match result {
                    Ok(resumed) => {
                        entity.accept_session(
                            resumed,
                            Some(PlayActionKind::Resume),
                            false,
                            true,
                            cx,
                        );
                    }
                    Err(err) => {
                        entity.accept_session(fallback_session, None, false, true, cx);
                        let parsed = PlaySessionError::from_domain(current_locale(cx), &err);
                        entity.error = Some(parsed.clone());
                        cx.emit(PlaySessionEvent::Error(parsed));
                        cx.notify();
                    }
                }
            });
        });
        self.in_flight = InFlight::Mutation {
            _observer: observer,
        };
        cx.notify();
    }

    /// Retries loading the most recently attempted or stored session.
    pub fn retry(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let stored = cx
            .try_global::<LocalUiState>()
            .and_then(|s| s.play_session_id);
        let Some(id) = self.retry_session_id.or(stored) else {
            return false;
        };
        self.load_session(id, false, false, window, cx)
    }

    /// Creates a new match session via non-abortable `TokioBridge::spawn_write`.
    pub fn create_session(
        &mut self,
        request: CreatePlaySession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.spawn_mutation(
            None,
            false,
            false,
            move |facade| async move { facade.create_play_session(request).await },
            window,
            cx,
        )
    }

    /// Executes an in-match action (`Roll`, `Move`, `Pass`, `Double`, `Take`, `Drop`,
    /// `Resign`, `ForfeitMatch`, `NextGame`, `EndSession`, `Pause`, `Resume`,
    /// `OpeningRoll`, `StartOpeningMove`) via non-abortable `TokioBridge::spawn_write`.
    #[allow(clippy::too_many_arguments)]
    pub fn perform_action(
        &mut self,
        action: PlayActionKind,
        move_index: Option<usize>,
        decision_id: Option<Uuid>,
        resign_level: Option<i32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(current) = self.session.as_ref() else {
            return false;
        };
        let session_id = current.id;
        let version = current.version;
        let req = PlayActionRequest {
            decision_id,
            version,
            action: action.to_backend(),
            move_index,
            resign_level,
        };
        self.spawn_mutation(
            Some(action),
            false,
            false,
            move |facade| async move { facade.act_in_play_session(session_id, req).await },
            window,
            cx,
        )
    }

    /// Changes live `PlayMode` and/or `tutor_threshold` while **preserving uncommitted steps**
    /// (`preserve_steps = true`). Emits `SwitchedToCompetition` when entering Competition mode.
    pub fn change_mode(
        &mut self,
        play_mode: PlayMode,
        tutor_threshold: Option<f64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(current) = self.session.as_ref() else {
            return false;
        };
        let session_id = current.id;
        let version = current.version;
        let switched_to_competition =
            play_mode == PlayMode::Competition && current.play_mode != PlayMode::Competition;
        let req = PlayModeRequest {
            version,
            play_mode,
            tutor_threshold,
        };
        self.spawn_mutation(
            None,
            true,
            switched_to_competition,
            move |facade| async move { facade.change_play_mode(session_id, req).await },
            window,
            cx,
        )
    }

    /// Requests a feedback operation (`Hint`, `Reveal`, `Retry`).
    /// `Hint` preserves uncommitted checker steps (`preserve_steps = true`).
    pub fn update_feedback(
        &mut self,
        operation: FeedbackOperation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(current) = self.session.as_ref() else {
            return false;
        };
        let session_id = current.id;
        let version = current.version;
        let decision_id = current.pending_decision.as_ref().map(|d| d.id);
        let preserve_steps = matches!(operation, FeedbackOperation::Hint);
        let req = PlayFeedbackRequest {
            version,
            operation,
            decision_id,
        };
        self.spawn_mutation(
            None,
            preserve_steps,
            false,
            move |facade| async move { facade.update_play_feedback(session_id, req).await },
            window,
            cx,
        )
    }

    /// Computes/retrieves the post-game / post-match learning review via
    /// non-abortable `TokioBridge::spawn_write`.
    pub fn review_session(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(current) = self.session.as_ref() else {
            return false;
        };
        let session_id = current.id;
        let version = current.version;
        let req = PlayReviewRequest { version };
        self.spawn_mutation(
            None,
            true,
            false,
            move |facade| async move { facade.review_play_session(session_id, req).await },
            window,
            cx,
        )
    }

    /// Core single-flight non-abortable mutation runner.
    fn spawn_mutation<F, Fut>(
        &mut self,
        action: Option<PlayActionKind>,
        preserve_steps: bool,
        emit_competition_switch: bool,
        work: F,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool
    where
        F: FnOnce(BackendFacade) -> Fut + Send + 'static,
        Fut: Future<Output = Result<PlaySession, DomainError>> + 'static,
    {
        if !self.can_start_mutation() {
            return false;
        }
        let Some(facade) = cx
            .try_global::<DesktopAppContext>()
            .and_then(|ctx| ctx.facade())
        else {
            return false;
        };

        self.error = None;
        self.generation = self.generation.wrapping_add(1);
        let token = self.generation;

        let write_task = TokioBridge::spawn_write(cx, run_facade(facade, work));
        let observer = cx.spawn_in(window, async move |this, cx| {
            let Ok(result) = write_task.await else {
                return;
            };
            let _ = this.update_in(cx, |entity, _window, cx| {
                if entity.generation != token {
                    return;
                }
                entity.in_flight = InFlight::None;
                match result {
                    Ok(updated) => {
                        let competition_match_id = if emit_competition_switch
                            && updated.play_mode == PlayMode::Competition
                        {
                            updated.library_match_id
                        } else {
                            None
                        };
                        entity.accept_session(updated, action, preserve_steps, false, cx);
                        if let Some(match_id) = competition_match_id {
                            cx.emit(PlaySessionEvent::SwitchedToCompetition { match_id });
                        }
                    }
                    Err(err) => {
                        let parsed = PlaySessionError::from_domain(current_locale(cx), &err);
                        entity.error = Some(parsed.clone());
                        cx.emit(PlaySessionEvent::Error(parsed));
                        cx.notify();
                    }
                }
            });
        });

        self.in_flight = InFlight::Mutation {
            _observer: observer,
        };
        cx.notify();
        true
    }

    fn accept_session(
        &mut self,
        next: PlaySession,
        action: Option<PlayActionKind>,
        preserve_steps: bool,
        from_load: bool,
        cx: &mut Context<Self>,
    ) {
        let previous = self.session.replace(next.clone());
        let same_session = previous.as_ref().is_some_and(|p| p.id == next.id);
        let same_game =
            same_session && previous.as_ref().is_some_and(|p| p.game_no == next.game_no);
        self.error = None;
        self.retry_session_id = Some(next.id);
        self.persist_session_id(Some(next.id), cx);

        cx.emit(PlaySessionEvent::Accepted(Box::new(AcceptedSession {
            session: next,
            previous,
            action,
            same_session,
            same_game,
            preserve_steps,
            from_load,
        })));
        cx.emit(PlaySessionEvent::LibraryMutated);
        cx.notify();
    }

    fn persist_session_id(&self, id: Option<Uuid>, cx: &mut App) {
        let Some(mut state) = cx.try_global::<LocalUiState>().cloned() else {
            return;
        };
        if state.play_session_id == id {
            return;
        }
        state.play_session_id = id;
        let path = self
            .persist_path
            .clone()
            .unwrap_or_else(LocalUiState::default_path);
        let _ = state.save_atomic(&path);
        cx.set_global(state);
    }

    fn forget_stored_session(&self, deleted_id: Uuid, cx: &mut App) {
        let matches_stored = cx
            .try_global::<LocalUiState>()
            .is_some_and(|s| s.play_session_id == Some(deleted_id));
        if matches_stored {
            self.persist_session_id(None, cx);
        }
    }
}
