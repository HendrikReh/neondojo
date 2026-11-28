//! Frame-accurate board animation state machine driven by `Instant` and GPUI's
//! `window.request_animation_frame()` with 0% idle GPU usage.
//!
//! Matches the exact timing, easing, reduced-motion, and sound-dispatch rules
//! from `web/src/shared/board/play-animation.ts`, `usePlayAnimation.ts`, and
//! `workspaces/play/play.css`.

use crate::audio::{PlaySoundKind, checker_sound};
use crate::geometry::{CheckerMotion, apply_absolute_steps};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Dice face cycle interval during tumble (`65 ms`).
pub const DICE_TICK_MS: u64 = 65;
/// Number of synthetic rolling faces shown before landing (standard motion).
pub const DICE_ROLL_TICKS: usize = 6;
/// Full tumble oscillation period (`180 ms`).
pub const DICE_TUMBLE_PERIOD_MS: f32 = 180.0;
/// Settle duration after an opening roll (`440 ms`).
pub const DICE_SETTLE_OPENING_MS: u64 = 440;
/// Settle duration after a regular roll (`240 ms`).
pub const DICE_SETTLE_NORMAL_MS: u64 = 240;
/// Settle duration when displaying non-rolling opening bot dice (`600 ms`).
pub const DICE_SETTLE_STATIC_MS: u64 = 600;
/// Checker flight duration (`280 ms` cubic-bezier(0.2, 0.65, 0.3, 1.0)).
pub const CHECKER_FLIGHT_MS: u64 = 280;
/// Post-landing pause before advancing to the next frame (`45 ms`).
pub const CHECKER_PAUSE_MS: u64 = 45;
/// Reduced-motion fade duration (`80 ms`).
pub const REDUCED_MOTION_FADE_MS: u64 = 80;
/// Reduced-motion checker pause (`10 ms`).
pub const REDUCED_CHECKER_PAUSE_MS: u64 = 10;

/// Consumption state for a single physical die on the board.
///
/// Mirrors `PlayDice.tsx` (`false`/0 unused, `0.5` half-used double, `true`/1
/// fully used).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DieUsage {
    #[default]
    Unused,
    /// Left 50% faded to `0.28` opacity, right 50% at full `1.0` opacity.
    Half,
    /// Entire die faded to `0.28` opacity (`0.55` when hovered & actionable).
    Used,
}

impl DieUsage {
    /// Computes `[die_0_usage, die_1_usage]` from the active dice and played
    /// step dice list (`usePlayTurn.ts:68-70`).
    pub fn for_turn(dice: &[u8], played_dice: &[u8]) -> [Self; 2] {
        if dice.len() < 2 {
            return [Self::Unused, Self::Unused];
        }
        if dice[0] == dice[1] {
            let n = played_dice.len();
            let first = match n {
                0 => Self::Unused,
                1 => Self::Half,
                _ => Self::Used,
            };
            let second = match n {
                0..=2 => Self::Unused,
                3 => Self::Half,
                _ => Self::Used,
            };
            [first, second]
        } else {
            [
                if played_dice.contains(&dice[0]) {
                    Self::Used
                } else {
                    Self::Unused
                },
                if played_dice.contains(&dice[1]) {
                    Self::Used
                } else {
                    Self::Unused
                },
            ]
        }
    }
}

/// A single checker animation step (plus optional simultaneous hit-to-bar
/// motion).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckerFrame {
    pub before: [i32; 26],
    pub after: [i32; 26],
    pub player: u8,
    pub dice: Vec<u8>,
    pub motions: Vec<CheckerMotion>,
    pub sound: Option<PlaySoundKind>,
}

/// A dice roll / opening roll / static dice display frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiceFrame {
    pub before: [i32; 26],
    pub player: u8,
    pub dice: Vec<u8>,
    pub roll: bool,
    pub opening: bool,
    pub tied: bool,
    pub sound: Option<PlaySoundKind>,
}

/// Queued playback unit in `BoardAnimationState`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayFrame {
    Checker(CheckerFrame),
    Dice(DiceFrame),
}

impl PlayFrame {
    pub fn sound(&self) -> Option<PlaySoundKind> {
        match self {
            Self::Checker(f) => f.sound,
            Self::Dice(f) => f.sound,
        }
    }

    pub fn player(&self) -> u8 {
        match self {
            Self::Checker(f) => f.player,
            Self::Dice(f) => f.player,
        }
    }
}

/// Builds a `CheckerFrame` for a single step `(from, to)` by `player`,
/// automatically adding the simultaneous opponent blot-to-bar motion when a hit
/// occurs (`play-animation.ts::checkerFrame`).
pub fn checker_frame(
    board: &[i32; 26],
    from: i32,
    to: i32,
    player: u8,
    dice: Vec<u8>,
) -> CheckerFrame {
    let sign = if player == 0 { 1 } else { -1 };
    let mut motions = vec![CheckerMotion { from, to, player }];
    if (1..=24).contains(&to) && board[to as usize] == -sign {
        motions.push(CheckerMotion {
            from: to,
            to: if player == 0 { 0 } else { 25 },
            player: 1 - player,
        });
    }
    let after = apply_absolute_steps(board, &[(from, to)], player);
    let sound = Some(checker_sound(board, to, player));
    CheckerFrame {
        before: *board,
        after,
        player,
        dice,
        motions,
        sound,
    }
}

/// Reverses a `CheckerFrame` for silent undo animation, including restoring any
/// hit blot from the bar (`play-animation.ts::reverseFrame`).
pub fn reverse_frame(frame: &CheckerFrame) -> CheckerFrame {
    CheckerFrame {
        before: frame.after,
        after: frame.before,
        player: frame.player,
        dice: frame.dice.clone(),
        motions: frame
            .motions
            .iter()
            .map(|m| CheckerMotion {
                from: m.to,
                to: m.from,
                player: m.player,
            })
            .collect(),
        sound: None,
    }
}

/// Deterministic synthetic dice faces during rolling ticks
/// (`usePlayAnimation.ts:142`).
pub const fn rolling_faces_for_tick(tick: usize) -> [u8; 2] {
    [
        (((tick * 3 + 1) % 6) + 1) as u8,
        (((tick * 5 + 3) % 6) + 1) as u8,
    ]
}

/// Evaluates CSS `cubic-bezier(0.2, 0.65, 0.3, 1.0)` at normalized time
/// `t ∈ [0.0, 1.0]` using Newton-Raphson with bisection fallback.
pub fn cubic_bezier_checker(t: f32) -> f32 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    const X1: f32 = 0.2;
    const Y1: f32 = 0.65;
    const X2: f32 = 0.3;
    const Y2: f32 = 1.0;

    let sample_x = |u: f32| -> f32 {
        let inv = 1.0 - u;
        3.0 * inv * inv * u * X1 + 3.0 * inv * u * u * X2 + u * u * u
    };
    let sample_dx = |u: f32| -> f32 {
        let inv = 1.0 - u;
        3.0 * inv * inv * X1 + 6.0 * inv * u * (X2 - X1) + 3.0 * u * u * (1.0 - X2)
    };
    let sample_y = |u: f32| -> f32 {
        let inv = 1.0 - u;
        3.0 * inv * inv * u * Y1 + 3.0 * inv * u * u * Y2 + u * u * u
    };

    let mut u = t;
    for _ in 0..8 {
        let err = sample_x(u) - t;
        if err.abs() < 1e-6 {
            return sample_y(u);
        }
        let d = sample_dx(u);
        if d.abs() < 1e-6 {
            break;
        }
        u = (u - err / d).clamp(0.0, 1.0);
    }

    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    for _ in 0..16 {
        let mid = 0.5 * (lo + hi);
        if sample_x(mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    sample_y(0.5 * (lo + hi))
}

/// Visual state sampled at an instant for the board renderer.
#[derive(Clone, Debug, PartialEq)]
pub enum ActiveVisual {
    Checker {
        before: [i32; 26],
        after: [i32; 26],
        player: u8,
        dice: Vec<u8>,
        motions: Vec<CheckerMotion>,
        /// Interpolated position progress `0.0..=1.0` along the flight path.
        progress: f32,
        /// Piece opacity (`1.0` in standard motion; `0.25..=1.0` under reduced motion).
        opacity: f32,
    },
    Dice {
        before: [i32; 26],
        player: u8,
        dice: Vec<u8>,
        opening: bool,
        rolling: bool,
        tied: bool,
        /// Logical pixel tumble offset `(dx, dy)` (`±4px, ±3px`).
        tumble_offset: (f32, f32),
        /// Tumble angle in radians (`±16°`).
        tumble_angle_rad: f32,
        /// Die opacity (`1.0` normally; `0.4..=1.0` under reduced motion).
        opacity: f32,
    },
}

/// Output of sampling `BoardAnimationState` for a single paint frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimationTick {
    /// Active animation frame to draw, or `None` if idle.
    pub visual: Option<ActiveVisual>,
    /// Sound cues triggered by newly started frames during this tick.
    pub sounds: Vec<PlaySoundKind>,
    /// True while another animation frame must be requested via
    /// `window.request_animation_frame()`.
    pub needs_next_frame: bool,
}

struct RunningFrame {
    frame: PlayFrame,
    started_at: Instant,
    sound_emitted: bool,
}

/// Instant-driven animation queue for dice rolls, checker flights, hits, and
/// undo reversals.
///
/// Consumes 0% CPU/GPU when idle (`is_active() == false`) because `BoardView`
/// only calls `window.request_animation_frame()` when `needs_next_frame` is
/// true.
pub struct BoardAnimationState {
    queue: VecDeque<PlayFrame>,
    current: Option<RunningFrame>,
    pending_roll_started: Option<Instant>,
    reduced_motion: bool,
    generation: u64,
}

impl Default for BoardAnimationState {
    fn default() -> Self {
        Self::new(false)
    }
}

impl BoardAnimationState {
    pub fn new(reduced_motion: bool) -> Self {
        Self {
            queue: VecDeque::new(),
            current: None,
            pending_roll_started: None,
            reduced_motion,
            generation: 0,
        }
    }

    pub fn set_reduced_motion(&mut self, reduced: bool) {
        self.reduced_motion = reduced;
    }

    pub fn reduced_motion(&self) -> bool {
        self.reduced_motion
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// True if any animation or pending server roll requires VSync repaints.
    pub fn is_active(&self) -> bool {
        self.current.is_some() || !self.queue.is_empty() || self.pending_roll_started.is_some()
    }

    /// Enqueues a batch of animation frames to play sequentially.
    pub fn enqueue(&mut self, frames: impl IntoIterator<Item = PlayFrame>, now: Instant) {
        self.queue.extend(frames);
        if self.current.is_none() {
            self.advance_locked(now);
        }
    }

    /// Starts or clears the indeterminate rolling-dice spinner while waiting
    /// for a server roll response.
    pub fn set_pending_roll(&mut self, active: bool, now: Instant) {
        self.pending_roll_started = active.then_some(now);
    }

    /// Returns synthetic dice faces for an active pending server roll, if any.
    pub fn pending_roll_faces(&self, now: Instant) -> Option<[u8; 2]> {
        let started = self.pending_roll_started?;
        let elapsed_ms = now.saturating_duration_since(started).as_millis() as u64;
        let tick = (elapsed_ms / DICE_TICK_MS) as usize;
        Some(rolling_faces_for_tick(tick))
    }

    /// Immediately cancels all active and queued animations (never alters
    /// authoritative game state).
    pub fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.queue.clear();
        self.current = None;
        self.pending_roll_started = None;
    }

    /// Advances the timeline to `now` and produces the current frame visual +
    /// any newly triggered audio cues.
    pub fn sample(&mut self, now: Instant) -> AnimationTick {
        let mut sounds = Vec::new();

        loop {
            if self.current.is_none() {
                self.advance_locked(now);
            }
            let Some(running) = self.current.as_mut() else {
                break;
            };
            if !running.sound_emitted {
                running.sound_emitted = true;
                if let Some(sound) = running.frame.sound() {
                    sounds.push(sound);
                }
            }
            let total_dur = frame_total_duration(&running.frame, self.reduced_motion);
            let elapsed = now.saturating_duration_since(running.started_at);
            if elapsed >= total_dur {
                let next_start = running.started_at + total_dur;
                self.current = None;
                self.advance_locked(next_start.min(now));
                continue;
            }
            let visual = evaluate_frame(&running.frame, elapsed, self.reduced_motion);
            return AnimationTick {
                visual: Some(visual),
                sounds,
                needs_next_frame: true,
            };
        }

        AnimationTick {
            visual: None,
            sounds,
            needs_next_frame: self.pending_roll_started.is_some(),
        }
    }

    fn advance_locked(&mut self, start_time: Instant) {
        if let Some(next) = self.queue.pop_front() {
            self.current = Some(RunningFrame {
                frame: next,
                started_at: start_time,
                sound_emitted: false,
            });
        }
    }
}

fn dice_roll_duration(frame: &DiceFrame, reduced: bool) -> Duration {
    if !frame.roll {
        return Duration::ZERO;
    }
    if reduced {
        Duration::from_millis(REDUCED_MOTION_FADE_MS)
    } else {
        Duration::from_millis(DICE_TICK_MS * DICE_ROLL_TICKS as u64)
    }
}

fn dice_settle_duration(frame: &DiceFrame, reduced: bool) -> Duration {
    let ms = if reduced {
        if frame.roll && !frame.opening {
            100
        } else {
            180
        }
    } else if frame.roll {
        if frame.opening {
            DICE_SETTLE_OPENING_MS
        } else {
            DICE_SETTLE_NORMAL_MS
        }
    } else {
        DICE_SETTLE_STATIC_MS
    };
    Duration::from_millis(ms)
}

fn frame_total_duration(frame: &PlayFrame, reduced: bool) -> Duration {
    match frame {
        PlayFrame::Dice(d) => dice_roll_duration(d, reduced) + dice_settle_duration(d, reduced),
        PlayFrame::Checker(_) => {
            if reduced {
                Duration::from_millis(REDUCED_MOTION_FADE_MS + REDUCED_CHECKER_PAUSE_MS)
            } else {
                Duration::from_millis(CHECKER_FLIGHT_MS + CHECKER_PAUSE_MS)
            }
        }
    }
}

fn evaluate_frame(frame: &PlayFrame, elapsed: Duration, reduced: bool) -> ActiveVisual {
    match frame {
        PlayFrame::Dice(d) => {
            let roll_dur = dice_roll_duration(d, reduced);
            if d.roll && elapsed < roll_dur {
                let elapsed_ms = elapsed.as_secs_f32() * 1000.0;
                let tick = (elapsed.as_millis() as u64 / DICE_TICK_MS) as usize;
                let faces = rolling_faces_for_tick(tick).to_vec();
                if reduced {
                    let frac = (elapsed_ms / REDUCED_MOTION_FADE_MS as f32).clamp(0.0, 1.0);
                    ActiveVisual::Dice {
                        before: d.before,
                        player: d.player,
                        dice: faces,
                        opening: d.opening,
                        rolling: true,
                        tied: d.tied,
                        tumble_offset: (0.0, 0.0),
                        tumble_angle_rad: 0.0,
                        opacity: 0.4 + 0.6 * frac,
                    }
                } else {
                    // Triangle wave matching @keyframes play-dice-tumble (180ms linear infinite).
                    let phase = (elapsed_ms % DICE_TUMBLE_PERIOD_MS) / DICE_TUMBLE_PERIOD_MS;
                    let tri = if phase < 0.5 {
                        phase * 2.0
                    } else {
                        (1.0 - phase) * 2.0
                    };
                    let signed = -1.0 + 2.0 * tri; // -1 -> +1 -> -1
                    ActiveVisual::Dice {
                        before: d.before,
                        player: d.player,
                        dice: faces,
                        opening: d.opening,
                        rolling: true,
                        tied: d.tied,
                        tumble_offset: (4.0 * signed, 3.0 * signed),
                        tumble_angle_rad: (16.0_f32 * signed).to_radians(),
                        opacity: 1.0,
                    }
                }
            } else {
                ActiveVisual::Dice {
                    before: d.before,
                    player: d.player,
                    dice: d.dice.clone(),
                    opening: d.opening,
                    rolling: false,
                    tied: d.tied,
                    tumble_offset: (0.0, 0.0),
                    tumble_angle_rad: 0.0,
                    opacity: 1.0,
                }
            }
        }
        PlayFrame::Checker(c) => {
            let flight_ms = if reduced {
                REDUCED_MOTION_FADE_MS
            } else {
                CHECKER_FLIGHT_MS
            };
            let flight_dur = Duration::from_millis(flight_ms);
            if elapsed < flight_dur {
                let raw_t = (elapsed.as_secs_f32() * 1000.0 / flight_ms as f32).clamp(0.0, 1.0);
                if reduced {
                    ActiveVisual::Checker {
                        before: c.before,
                        after: c.after,
                        player: c.player,
                        dice: c.dice.clone(),
                        motions: c.motions.clone(),
                        progress: 1.0,
                        opacity: 0.25 + 0.75 * raw_t,
                    }
                } else {
                    ActiveVisual::Checker {
                        before: c.before,
                        after: c.after,
                        player: c.player,
                        dice: c.dice.clone(),
                        motions: c.motions.clone(),
                        progress: cubic_bezier_checker(raw_t),
                        opacity: 1.0,
                    }
                }
            } else {
                // Post-land pause: show final board with no in-flight pieces.
                ActiveVisual::Checker {
                    before: c.after,
                    after: c.after,
                    player: c.player,
                    dice: c.dice.clone(),
                    motions: Vec::new(),
                    progress: 1.0,
                    opacity: 1.0,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubic_bezier_is_monotonic_and_matches_endpoints() {
        assert_eq!(cubic_bezier_checker(0.0), 0.0);
        assert_eq!(cubic_bezier_checker(1.0), 1.0);
        let mut prev = 0.0;
        for step in 1..=20 {
            let t = step as f32 / 20.0;
            let y = cubic_bezier_checker(t);
            assert!(y >= prev && y <= 1.0001);
            prev = y;
        }
        // Fast start ease-out: midpoint should be well past 0.5.
        assert!(cubic_bezier_checker(0.5) > 0.75);
    }

    #[test]
    fn checker_frame_handles_hit_bear_off_and_silent_reverse() {
        let mut b = [0i32; 26];
        b[25] = 1;
        b[24] = -1;
        let hit = checker_frame(&b, 25, 24, 0, vec![1, 2]);
        assert_eq!(hit.motions.len(), 2);
        assert_eq!(
            hit.motions[1],
            CheckerMotion {
                from: 24,
                to: 0,
                player: 1
            }
        );
        assert_eq!(hit.sound, Some(PlaySoundKind::Hit));
        assert_eq!(hit.after[25], 0);
        assert_eq!(hit.after[24], 1);
        assert_eq!(hit.after[0], -1);

        let rev = reverse_frame(&hit);
        assert_eq!(rev.before, hit.after);
        assert_eq!(rev.after, b);
        assert_eq!(rev.sound, None);
        assert_eq!(
            rev.motions[0],
            CheckerMotion {
                from: 24,
                to: 25,
                player: 0
            }
        );
        assert_eq!(
            rev.motions[1],
            CheckerMotion {
                from: 0,
                to: 24,
                player: 1
            }
        );
    }

    #[test]
    fn timeline_advances_dice_to_checker_emits_sounds_once_and_goes_idle() {
        let t0 = Instant::now();
        let mut state = BoardAnimationState::new(false);
        assert!(!state.is_active());

        let mut b = [0i32; 26];
        b[6] = 2;
        state.enqueue(
            [
                PlayFrame::Dice(DiceFrame {
                    before: b,
                    player: 0,
                    dice: vec![3, 1],
                    roll: true,
                    opening: false,
                    tied: false,
                    sound: Some(PlaySoundKind::Roll),
                }),
                PlayFrame::Checker(checker_frame(&b, 6, 3, 0, vec![3, 1])),
            ],
            t0,
        );

        // Start of dice roll: sound fires once, dice rolling.
        let tick0 = state.sample(t0);
        assert_eq!(tick0.sounds, vec![PlaySoundKind::Roll]);
        assert!(tick0.needs_next_frame);
        assert!(matches!(
            tick0.visual,
            Some(ActiveVisual::Dice { rolling: true, .. })
        ));

        // Mid-roll (130ms): no duplicate roll sound, still rolling.
        let tick1 = state.sample(t0 + Duration::from_millis(130));
        assert!(tick1.sounds.is_empty());
        assert!(matches!(
            tick1.visual,
            Some(ActiveVisual::Dice { rolling: true, .. })
        ));

        // Settle (450ms: past 390ms roll, before 630ms end): settled with final [3, 1].
        let tick2 = state.sample(t0 + Duration::from_millis(450));
        assert!(matches!(
            tick2.visual,
            Some(ActiveVisual::Dice { rolling: false, ref dice, .. }) if dice == &[3, 1]
        ));

        // Checker flight (700ms: 70ms into checker frame): emits Move sound once.
        let tick3 = state.sample(t0 + Duration::from_millis(700));
        assert_eq!(tick3.sounds, vec![PlaySoundKind::Move]);
        assert!(matches!(
            tick3.visual,
            Some(ActiveVisual::Checker { progress, .. }) if progress > 0.0 && progress < 1.0
        ));

        // Completed (1000ms: past 630 + 325 = 955ms): idle, 0% GPU.
        let tick4 = state.sample(t0 + Duration::from_millis(1_000));
        assert!(tick4.visual.is_none());
        assert!(!tick4.needs_next_frame);
        assert!(!state.is_active());
    }

    #[test]
    fn reduced_motion_uses_80ms_fade_and_half_used_doubles_work() {
        let t0 = Instant::now();
        let mut state = BoardAnimationState::new(true);
        let mut b = [0i32; 26];
        b[6] = 1;
        state.enqueue(
            [PlayFrame::Checker(checker_frame(&b, 6, 4, 0, vec![2, 2]))],
            t0,
        );

        let mid = state.sample(t0 + Duration::from_millis(40));
        match mid.visual {
            Some(ActiveVisual::Checker {
                progress, opacity, ..
            }) => {
                assert_eq!(progress, 1.0);
                assert!(opacity > 0.5 && opacity < 0.7);
            }
            other => panic!("expected reduced checker visual, got {other:?}"),
        }

        // Past 80ms fade + 10ms pause -> idle.
        let end = state.sample(t0 + Duration::from_millis(95));
        assert!(!end.needs_next_frame);

        assert_eq!(
            DieUsage::for_turn(&[4, 4], &[]),
            [DieUsage::Unused, DieUsage::Unused]
        );
        assert_eq!(
            DieUsage::for_turn(&[4, 4], &[4]),
            [DieUsage::Half, DieUsage::Unused]
        );
        assert_eq!(
            DieUsage::for_turn(&[4, 4], &[4, 4]),
            [DieUsage::Used, DieUsage::Unused]
        );
        assert_eq!(
            DieUsage::for_turn(&[4, 4], &[4, 4, 4]),
            [DieUsage::Used, DieUsage::Half]
        );
        assert_eq!(
            DieUsage::for_turn(&[4, 4], &[4, 4, 4, 4]),
            [DieUsage::Used, DieUsage::Used]
        );
    }
}
