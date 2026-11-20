import type { PlaySound } from './play-sound-events';
import moveRecording from './assets/play-sounds/checker-move.wav';
import rollRecording from './assets/play-sounds/rolling-dice.wav';
import hitRecording from './assets/play-sounds/checker-hit.wav';
import cubeRecording from './assets/play-sounds/cube-double.wav';
export const playSoundFiles: Record<PlaySound, string> = {
  move: moveRecording,
  roll: rollRecording,
  hit: hitRecording,
  bear_off: moveRecording,
  cube: cubeRecording,
};
export const playSoundsStorageKey = 'neondojo.play-sounds';
let enabledInMemory = true;

export function readPlaySounds(storage?: Pick<Storage, 'getItem'>): boolean {
  try {
    const value = (storage ?? localStorage).getItem(playSoundsStorageKey);
    return value === 'off' ? false : value === 'on' ? true : enabledInMemory;
  } catch {
    return enabledInMemory;
  }
}
export function savePlaySounds(
  enabled: boolean,
  storage?: Pick<Storage, 'setItem'>,
) {
  enabledInMemory = enabled;
  try {
    (storage ?? localStorage).setItem(
      playSoundsStorageKey,
      enabled ? 'on' : 'off',
    );
  } catch {
    // This browser session still honors the preference without storage.
  }
  if (!enabled) stopPlaySounds();
}

let context: AudioContext | undefined;
let ready: Promise<void> | undefined;
let generation = 0;
let output: GainNode | undefined;
const buffers = new Map<string, Promise<AudioBuffer>>();
function recordedBuffer(
  audio: AudioContext,
  kind: PlaySound,
): Promise<AudioBuffer> {
  const url = playSoundFiles[kind];
  const cached = buffers.get(url);
  if (cached) return cached;
  const loading = fetch(url)
    .then((response) => {
      if (!response.ok)
        throw new Error(`Sound unavailable: ${response.status}`);
      return response.arrayBuffer();
    })
    .then((data) => audio.decodeAudioData(data));
  buffers.set(url, loading);
  void loading.catch(() => {
    if (buffers.get(url) === loading) buffers.delete(url);
  });
  return loading;
}
const active = new Set<AudioBufferSourceNode>();

/** Must run in the actual click/key handler, before waiting for the engine. */
export function unlockPlayAudio() {
  if (!readPlaySounds()) return;
  try {
    if (!context || context.state === 'closed') {
      context = new AudioContext();
      output = context.createGain();
      output.gain.value = 0.8;
      output.connect(context.destination);
      buffers.clear();
    }
    ready = context.state === 'running' ? Promise.resolve() : context.resume();
    void ready.catch(() => {});
    // Load/decode the small recordings once, starting on a user gesture.
    for (const kind of ['move', 'roll', 'hit', 'cube'] as const) {
      void recordedBuffer(context, kind).catch(() => {});
    }
  } catch {
    // Audio support and playback permissions never gate a legal game action.
  }
}
export function stopPlaySounds() {
  ++generation;
  for (const source of active) {
    try {
      source.stop();
    } catch {
      /* Already ended. */
    }
    source.disconnect();
  }
  active.clear();
}
export function playSounds(sounds: PlaySound[]) {
  if (
    !readPlaySounds() ||
    !context ||
    !ready ||
    document.hidden ||
    !sounds.length
  )
    return;
  const audio = context,
    token = generation;
  // Wait for gesture unlock without retaining cues after mute/workspace changes.
  void ready
    .then(async () => {
      if (
        token !== generation ||
        !readPlaySounds() ||
        document.hidden ||
        audio.state !== 'running'
      )
        return;
      const decoded = await Promise.all(
        sounds.map(async (kind) => ({
          kind,
          buffer: await recordedBuffer(audio, kind).catch(() => undefined),
        })),
      );
      // A mute, hidden workspace or new audio context may occur during I/O.
      if (
        token !== generation ||
        !readPlaySounds() ||
        document.hidden ||
        audio !== context ||
        audio.state !== 'running'
      )
        return;
      try {
        let start = audio.currentTime + 0.005;
        for (const { buffer } of decoded) {
          if (!buffer) continue;
          const source = audio.createBufferSource();
          source.buffer = buffer;
          source.connect(output ?? audio.destination);
          source.onended = () => {
            active.delete(source);
            source.disconnect();
          };
          active.add(source);
          source.start(start);
          start += buffer.duration;
        }
      } catch {
        stopPlaySounds();
      }
    })
    .catch(() => {
      // Denied playback/device access drops this cue; a future click may retry.
    });
}
