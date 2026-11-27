//! Native game sound effects backed by `rodio` 0.21 with pre-decoded embedded
//! PCM WAV recordings from `assets/play-sounds/`.
//!
//! Playback runs on a dedicated audio worker thread using a single sequential
//! `rodio::Sink` at fixed `0.8` gain so multi-step cues (e.g. bot roll → hit →
//! move) play end-to-end without overlap, clear immediately on mute or window
//! deactivation, and degrade to a silent no-op when no audio output device is
//! available (such as headless CI).

use anyhow::{Context, Result};
use rodio::{Decoder, OutputStreamBuilder, Sink, Source, buffer::SamplesBuffer};
use std::io::Cursor;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Sender},
};

/// Default playback volume matching `web/src/shared/board/play-audio.ts`.
pub const DEFAULT_PLAY_SOUND_VOLUME: f32 = 0.8;

const CHECKER_MOVE_WAV: &[u8] = include_bytes!("../assets/play-sounds/checker-move.wav");
const ROLLING_DICE_WAV: &[u8] = include_bytes!("../assets/play-sounds/rolling-dice.wav");
const CHECKER_HIT_WAV: &[u8] = include_bytes!("../assets/play-sounds/checker-hit.wav");
const CUBE_DOUBLE_WAV: &[u8] = include_bytes!("../assets/play-sounds/cube-double.wav");

/// Logical play sound cues matching `web/src/shared/board/play-sound-events.ts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaySoundKind {
    Move,
    Roll,
    Hit,
    BearOff,
    Cube,
}

impl PlaySoundKind {
    /// Returns the embedded canonical PCM WAV bytes for this cue.
    ///
    /// `BearOff` reuses `checker-move.wav` because no dedicated recording was
    /// supplied in `sounds/`.
    pub const fn wav_bytes(self) -> &'static [u8] {
        match self {
            Self::Move | Self::BearOff => CHECKER_MOVE_WAV,
            Self::Roll => ROLLING_DICE_WAV,
            Self::Hit => CHECKER_HIT_WAV,
            Self::Cube => CUBE_DOUBLE_WAV,
        }
    }
}

/// Pre-decoded mono/multi-channel floating-point PCM buffer ready for zero-I/O
/// playback through `rodio::SamplesBuffer`.
#[derive(Clone, Debug)]
pub struct DecodedClip {
    pub samples: Arc<[f32]>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl DecodedClip {
    /// Number of interleaved sample frames in this clip.
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            0
        } else {
            self.samples.len() / usize::from(self.channels)
        }
    }

    /// Root-mean-square amplitude across all decoded samples.
    pub fn rms(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let energy: f64 = self
            .samples
            .iter()
            .map(|&s| f64::from(s) * f64::from(s))
            .sum();
        (energy / self.samples.len() as f64).sqrt()
    }

    fn to_source(&self) -> SamplesBuffer {
        SamplesBuffer::new(self.channels, self.sample_rate, self.samples.to_vec())
    }
}

/// Decodes one of the 4 embedded PCM WAV recordings into a `DecodedClip`.
pub fn decode_embedded_wav(kind: PlaySoundKind) -> Result<DecodedClip> {
    let decoder = Decoder::new_wav(Cursor::new(kind.wav_bytes()))
        .with_context(|| format!("decode embedded WAV for {kind:?}"))?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<f32> = decoder.collect();
    Ok(DecodedClip {
        samples: Arc::from(samples),
        sample_rate,
        channels,
    })
}

/// Classifies the sound cue for a single checker step on `board`.
///
/// Matches `web/src/shared/board/play-sound-events.ts::checkerSound`.
pub fn checker_sound(board: &[i32; 26], to: i32, player: u8) -> PlaySoundKind {
    if to == -1 || to == 26 {
        return PlaySoundKind::BearOff;
    }
    if (1..=24).contains(&to) {
        let opponent = if player == 0 { -1 } else { 1 };
        if board[to as usize] == opponent {
            return PlaySoundKind::Hit;
        }
    }
    PlaySoundKind::Move
}

#[derive(Clone, Debug)]
struct DecodedBank {
    move_clip: DecodedClip,
    roll_clip: DecodedClip,
    hit_clip: DecodedClip,
    cube_clip: DecodedClip,
}

impl DecodedBank {
    fn load() -> Result<Self> {
        Ok(Self {
            move_clip: decode_embedded_wav(PlaySoundKind::Move)?,
            roll_clip: decode_embedded_wav(PlaySoundKind::Roll)?,
            hit_clip: decode_embedded_wav(PlaySoundKind::Hit)?,
            cube_clip: decode_embedded_wav(PlaySoundKind::Cube)?,
        })
    }

    fn clip(&self, kind: PlaySoundKind) -> &DecodedClip {
        match kind {
            PlaySoundKind::Move | PlaySoundKind::BearOff => &self.move_clip,
            PlaySoundKind::Roll => &self.roll_clip,
            PlaySoundKind::Hit => &self.hit_clip,
            PlaySoundKind::Cube => &self.cube_clip,
        }
    }
}

enum AudioCommand {
    Play {
        generation: u64,
        sounds: Vec<PlaySoundKind>,
    },
    Clear,
    SetVolume(f32),
    Shutdown,
}

struct AudioShared {
    enabled: AtomicBool,
    window_active: AtomicBool,
    generation: AtomicU64,
    tx: Option<Sender<AudioCommand>>,
}

impl Drop for AudioShared {
    fn drop(&mut self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(AudioCommand::Shutdown);
        }
    }
}

/// Thread-safe handle to NeonDojo's native play-sound engine.
///
/// Cloning is cheap (`Arc`). Dropping the last handle shuts down the background
/// audio worker cleanly.
#[derive(Clone)]
pub struct PlayAudioEngine {
    shared: Arc<AudioShared>,
}

impl PlayAudioEngine {
    /// Starts the audio worker thread and pre-decodes all 4 embedded WAV clips.
    ///
    /// Never fails if audio hardware is unavailable; playback silently no-ops.
    pub fn new(enabled: bool) -> Self {
        let bank = match DecodedBank::load() {
            Ok(bank) => bank,
            Err(err) => {
                tracing::warn!("embedded play sounds failed to decode: {err:#}");
                return Self::silent(enabled);
            }
        };

        let (tx, rx) = mpsc::channel::<AudioCommand>();
        let shared = Arc::new(AudioShared {
            enabled: AtomicBool::new(enabled),
            window_active: AtomicBool::new(true),
            generation: AtomicU64::new(0),
            tx: Some(tx),
        });
        let worker_shared = Arc::clone(&shared);

        let spawn_result = std::thread::Builder::new()
            .name("neondojo-audio".to_string())
            .spawn(move || {
                audio_worker_loop(rx, bank, worker_shared);
            });

        if let Err(err) = spawn_result {
            tracing::warn!("failed to spawn audio worker thread: {err}");
            return Self::silent(enabled);
        }

        Self { shared }
    }

    /// Creates a silent no-op engine useful for headless tests and environments
    /// where opening CoreAudio output is unwanted.
    pub fn silent(enabled: bool) -> Self {
        Self {
            shared: Arc::new(AudioShared {
                enabled: AtomicBool::new(enabled),
                window_active: AtomicBool::new(true),
                generation: AtomicU64::new(0),
                tx: None,
            }),
        }
    }

    /// Returns whether game sounds are currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.shared.enabled.load(Ordering::Acquire)
    }

    /// Enables or disables game sounds immediately. Disabling increments the
    /// generation counter and clears any queued audio immediately.
    pub fn set_enabled(&self, enabled: bool) {
        self.shared.enabled.store(enabled, Ordering::Release);
        if !enabled {
            self.stop();
        }
    }

    /// Tracks whether the Play workspace / window is currently visible/active.
    /// Deactivation immediately stops and clears queued cues.
    pub fn set_window_active(&self, active: bool) {
        self.shared.window_active.store(active, Ordering::Release);
        if !active {
            self.stop();
        }
    }

    /// Returns the current cancellation generation counter.
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Acquire)
    }

    /// Queues one or more sounds sequentially at `DEFAULT_PLAY_SOUND_VOLUME`.
    pub fn play(&self, sounds: &[PlaySoundKind]) {
        if sounds.is_empty()
            || !self.shared.enabled.load(Ordering::Acquire)
            || !self.shared.window_active.load(Ordering::Acquire)
        {
            return;
        }
        if let Some(tx) = &self.shared.tx {
            let generation = self.shared.generation.load(Ordering::Acquire);
            let _ = tx.send(AudioCommand::Play {
                generation,
                sounds: sounds.to_vec(),
            });
        }
    }

    /// Convenience helper for playing a single sound cue.
    pub fn play_one(&self, sound: PlaySoundKind) {
        self.play(&[sound]);
    }

    /// Immediately stops playback and discards any queued sounds.
    pub fn stop(&self) {
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        if let Some(tx) = &self.shared.tx {
            let _ = tx.send(AudioCommand::Clear);
        }
    }

    /// Updates playback volume (clamped to `0.0..=1.0`).
    pub fn set_volume(&self, volume: f32) {
        if let Some(tx) = &self.shared.tx {
            let _ = tx.send(AudioCommand::SetVolume(volume.clamp(0.0, 1.0)));
        }
    }
}

fn audio_worker_loop(
    rx: mpsc::Receiver<AudioCommand>,
    bank: DecodedBank,
    shared: Arc<AudioShared>,
) {
    // Open the default output lazily on first Play so idle launches and
    // muted sessions never touch CoreAudio unnecessarily.
    let mut output: Option<(rodio::OutputStream, Sink)> = None;
    let mut output_failed = false;
    let mut volume = DEFAULT_PLAY_SOUND_VOLUME;

    while let Ok(cmd) = rx.recv() {
        match cmd {
            AudioCommand::Shutdown => {
                if let Some((_, sink)) = output.take() {
                    sink.stop();
                }
                break;
            }
            AudioCommand::Clear => {
                if let Some((_, sink)) = &output {
                    sink.clear();
                }
            }
            AudioCommand::SetVolume(v) => {
                volume = v;
                if let Some((_, sink)) = &output {
                    sink.set_volume(volume);
                }
            }
            AudioCommand::Play { generation, sounds } => {
                if generation != shared.generation.load(Ordering::Acquire)
                    || !shared.enabled.load(Ordering::Acquire)
                    || !shared.window_active.load(Ordering::Acquire)
                {
                    continue;
                }
                if output.is_none() && !output_failed {
                    match OutputStreamBuilder::open_default_stream() {
                        Ok(mut stream) => {
                            stream.log_on_drop(false);
                            let sink = Sink::connect_new(stream.mixer());
                            sink.set_volume(volume);
                            output = Some((stream, sink));
                        }
                        Err(err) => {
                            tracing::debug!("play audio unavailable (silent fallback): {err}");
                            output_failed = true;
                        }
                    }
                }
                if let Some((_, sink)) = &output {
                    for kind in sounds {
                        if generation != shared.generation.load(Ordering::Acquire) {
                            sink.clear();
                            break;
                        }
                        sink.append(bank.clip(kind).to_source());
                    }
                    sink.play();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_wav_clips_match_canonical_rates_frames_and_energy() {
        let expected = [
            (PlaySoundKind::Move, 8_012u32, 3_247usize),
            (PlaySoundKind::Roll, 8_012, 2_204),
            (PlaySoundKind::Hit, 22_000, 1_123),
            (PlaySoundKind::Cube, 8_012, 1_238),
            (PlaySoundKind::BearOff, 8_012, 3_247),
        ];

        for (kind, rate, frames) in expected {
            let clip = decode_embedded_wav(kind).expect("embedded wav decodes");
            assert_eq!(clip.channels, 1, "{kind:?} must be mono");
            assert_eq!(clip.sample_rate, rate, "{kind:?} sample rate mismatch");
            assert_eq!(clip.frame_count(), frames, "{kind:?} frame count mismatch");
            assert!(
                clip.rms() > 0.005,
                "{kind:?} RMS energy too low: {}",
                clip.rms()
            );
        }
    }

    #[test]
    fn bear_off_reuses_checker_move_bytes() {
        assert_eq!(
            PlaySoundKind::BearOff.wav_bytes(),
            PlaySoundKind::Move.wav_bytes()
        );
    }

    #[test]
    fn checker_sound_distinguishes_move_hit_and_bear_off() {
        let mut board = [0i32; 26];
        board[4] = -1;
        board[21] = 1;
        assert_eq!(checker_sound(&board, 5, 0), PlaySoundKind::Move);
        assert_eq!(checker_sound(&board, 4, 0), PlaySoundKind::Hit);
        assert_eq!(checker_sound(&board, -1, 0), PlaySoundKind::BearOff);
        assert_eq!(checker_sound(&board, 21, 1), PlaySoundKind::Hit);
        assert_eq!(checker_sound(&board, 26, 1), PlaySoundKind::BearOff);
        assert_eq!(checker_sound(&board, 21, 0), PlaySoundKind::Move);
    }

    #[test]
    fn silent_engine_tracks_enable_and_generation_without_panicking() {
        let engine = PlayAudioEngine::silent(true);
        assert!(engine.is_enabled());
        let gen0 = engine.generation();
        engine.play(&[PlaySoundKind::Roll, PlaySoundKind::Move]);
        assert_eq!(engine.generation(), gen0);
        engine.set_enabled(false);
        assert!(!engine.is_enabled());
        assert_eq!(engine.generation(), gen0 + 1);
        engine.set_window_active(false);
        assert_eq!(engine.generation(), gen0 + 2);
    }
}
