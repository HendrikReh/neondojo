//! Isolated 60–120fps board animation & native audio coordinator (`PlayAnimationEntity`).
//!
//! Only `PlayBoardStage` observes frame ticks from `PlayAnimationEntity`, ensuring
//! `PlayWorkspace`, `PlayHistory`, and match header panels NEVER re-render during
//! checker flights or dice rolls. Consumes 0% GPU when idle.
//!
//! Integrates `neondojo_board::PlayAudioEngine`, silencing audio immediately on:
//! - mute toggle (`set_sounds_enabled(false)`)
//! - workspace hide (`set_visible(false)`)
//! - session reload / cancel (`cancel`)
//! - `Z` undo (`reverse_frame` carries `sound: None`).

use crate::workspaces::play::interruption::PlayActionKind;
use gpui_kit::{Context, EventEmitter, Window};
use neondojo_board::{
    ActiveVisual, BoardAnimationState, PlayAudioEngine, PlayFrame, PlaySoundKind,
};
use std::time::Instant;

/// Lifecycle events emitted by [`PlayAnimationEntity`] to `PlayWorkspace`.
///
/// Emitted ONLY on state-machine transitions (`Started` / `Settled`), NEVER on
/// per-frame ticks — keeping `PlayWorkspace`, header, and `PlayHistory` completely
/// untouched during 60–120fps animations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayAnimationEvent {
    Started,
    Settled,
}

/// Isolated GPUI entity owning `BoardAnimationState` and `PlayAudioEngine`.
pub struct PlayAnimationEntity {
    state: BoardAnimationState,
    audio: PlayAudioEngine,
    visible: bool,
    pending_roll: Option<PlayActionKind>,
    current_visual: Option<ActiveVisual>,
    frame_scheduled: bool,
    /// Monotonic counter incremented when a fresh batch of frames is enqueued so
    /// `PlayBoardStage` knows when to forward frames to `BoardView`.
    batch_seq: u64,
    last_enqueued_batch: Vec<PlayFrame>,
    /// Tracks whether `PlayAnimationEvent::Started` was emitted without a matching `Settled`.
    notified_active: bool,
    /// Diagnostic counter of frame ticks (verified in tests to stay 0 when idle).
    tick_count: u64,
}

impl EventEmitter<PlayAnimationEvent> for PlayAnimationEntity {}

impl PlayAnimationEntity {
    pub fn new(sounds_enabled: bool) -> Self {
        Self::with_audio(PlayAudioEngine::new(sounds_enabled))
    }

    /// Creates an animation entity with a silent no-op audio backend (for headless tests).
    pub fn silent(sounds_enabled: bool) -> Self {
        Self::with_audio(PlayAudioEngine::silent(sounds_enabled))
    }

    fn with_audio(audio: PlayAudioEngine) -> Self {
        Self {
            state: BoardAnimationState::default(),
            audio,
            visible: true,
            pending_roll: None,
            current_visual: None,
            frame_scheduled: false,
            batch_seq: 0,
            last_enqueued_batch: Vec::new(),
            notified_active: false,
            tick_count: 0,
        }
    }

    pub fn is_animating(&self) -> bool {
        self.state.is_active() || self.current_visual.is_some()
    }

    pub fn is_sounds_enabled(&self) -> bool {
        self.audio.is_enabled()
    }

    pub fn audio_generation(&self) -> u64 {
        self.audio.generation()
    }

    pub fn pending_roll(&self) -> Option<PlayActionKind> {
        self.pending_roll
    }

    pub fn current_visual(&self) -> Option<&ActiveVisual> {
        self.current_visual.as_ref()
    }

    pub fn batch_seq(&self) -> u64 {
        self.batch_seq
    }

    pub fn last_enqueued_batch(&self) -> &[PlayFrame] {
        &self.last_enqueued_batch
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.state.set_reduced_motion(reduced);
    }

    /// Enables or mutes play sounds immediately (silencing queued audio on mute).
    pub fn set_sounds_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.audio.set_enabled(enabled);
        cx.notify();
    }

    /// Synchronizes workspace visibility: hiding immediately cancels active
    /// animations and silences all queued audio (`PlayWorkspace.tsx:76-79`).
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        self.visible = visible;
        self.audio.set_window_active(visible);
        if !visible {
            self.cancel_internal(cx);
        }
    }

    /// Plays a standalone cue (such as `PlaySoundKind::Cube` on a new bot double)
    /// only while visible and unmuted.
    pub fn play_cue(&self, kind: PlaySoundKind) {
        if self.visible {
            self.audio.play_one(kind);
        }
    }

    /// Starts or clears the indeterminate rolling-dice animation while waiting
    /// for a server roll response.
    pub fn set_pending_roll(
        &mut self,
        pending: Option<PlayActionKind>,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if self.pending_roll == pending {
            return;
        }
        self.pending_roll = pending;
        self.state
            .set_pending_roll(pending.is_some(), Instant::now());
        if pending.is_some() {
            if !self.notified_active {
                self.notified_active = true;
                cx.emit(PlayAnimationEvent::Started);
            }
            if let Some(win) = window {
                self.ensure_frame_loop(win, cx);
            }
        } else if !self.state.is_active() {
            self.current_visual = None;
            if self.notified_active {
                self.notified_active = false;
                cx.emit(PlayAnimationEvent::Settled);
            }
        }
        cx.notify();
    }

    /// Enqueues a batch of animation frames (`PlayFrame::Dice` / `PlayFrame::Checker`).
    ///
    /// Frames with `sound: None` (such as `Z` undo via `reverse_frame`) stay
    /// completely silent. If workspace is hidden, the batch is skipped silently.
    pub fn enqueue(
        &mut self,
        frames: Vec<PlayFrame>,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if frames.is_empty() || !self.visible {
            return;
        }
        let now = Instant::now();
        self.batch_seq = self.batch_seq.wrapping_add(1);
        self.last_enqueued_batch = frames.clone();
        self.state.enqueue(frames, now);

        // Immediately sample t=0 so the first visual & cue fire without 1-frame delay.
        let tick = self.state.sample(now);
        self.dispatch_tick_sounds(&tick.sounds);
        self.current_visual = tick.visual;

        if self.is_animating() {
            if !self.notified_active {
                self.notified_active = true;
                cx.emit(PlayAnimationEvent::Started);
            }
            if let Some(win) = window {
                self.ensure_frame_loop(win, cx);
            }
        } else if self.notified_active {
            self.notified_active = false;
            cx.emit(PlayAnimationEvent::Settled);
        }
        cx.notify();
    }

    /// Immediately cancels all running and queued animations and stops audio
    /// (called on reload, session switch, or workspace hide).
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        self.cancel_internal(cx);
    }

    fn cancel_internal(&mut self, cx: &mut Context<Self>) {
        let was_busy = self.is_animating() || self.pending_roll.is_some();
        self.state.cancel();
        self.audio.stop();
        self.pending_roll = None;
        self.current_visual = None;
        self.last_enqueued_batch.clear();
        self.batch_seq = self.batch_seq.wrapping_add(1);
        self.frame_scheduled = false;
        if self.notified_active {
            self.notified_active = false;
            cx.emit(PlayAnimationEvent::Settled);
        }
        if was_busy {
            cx.notify();
        }
    }

    /// Advances the internal animation timeline to `now` (usable directly in tests
    /// with deterministic synthetic `Instant` offsets).
    pub fn sample_at(&mut self, now: Instant, cx: &mut Context<Self>) -> bool {
        self.tick_count = self.tick_count.saturating_add(1);
        let tick = self.state.sample(now);
        self.dispatch_tick_sounds(&tick.sounds);
        self.current_visual = tick.visual;
        cx.notify();

        if !tick.needs_next_frame && !self.state.is_active() {
            self.current_visual = None;
            if self.notified_active {
                self.notified_active = false;
                cx.emit(PlayAnimationEvent::Settled);
            }
            return false;
        }
        true
    }

    fn dispatch_tick_sounds(&self, sounds: &[PlaySoundKind]) {
        if self.visible && !sounds.is_empty() {
            self.audio.play(sounds);
        }
    }

    fn ensure_frame_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.frame_scheduled || !self.state.is_active() {
            return;
        }
        self.frame_scheduled = true;
        let weak = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |this, cx| {
                this.frame_scheduled = false;
                if !this.visible {
                    this.cancel_internal(cx);
                    return;
                }
                let keep_going = this.sample_at(Instant::now(), cx);
                if keep_going {
                    this.ensure_frame_loop(window, cx);
                }
            });
        });
    }
}
