import { readFileSync } from 'node:fs';
import { expect, it, vi } from 'vitest';
import {
  readPlaySounds,
  savePlaySounds,
  playSounds,
  unlockPlayAudio,
  playSoundFiles,
} from './play-audio';
import { onPlaySoundsChanged } from './play-sound-preferences';
it('maps game events to the supplied recordings and reuses movement for bear-off', () => {
  expect(playSoundFiles.move).toMatch(/checker-move\.wav$/);
  expect(playSoundFiles.roll).toMatch(/rolling-dice\.wav$/);
  expect(playSoundFiles.hit).toMatch(/checker-hit\.wav$/);
  expect(playSoundFiles.bear_off).toBe(playSoundFiles.move);
});

it('stores mute and honors the in-memory preference when storage is blocked', () => {
  const values = new Map<string, string>();
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => {
      values.set(key, value);
    },
  };
  expect(readPlaySounds(storage)).toBe(true);
  savePlaySounds(false, storage);
  expect(values.get('neondojo.play-sounds')).toBe('off');
  expect(readPlaySounds(storage)).toBe(false);
  expect(
    readPlaySounds({
      getItem: () => {
        throw new Error('blocked');
      },
    }),
  ).toBe(false);
  savePlaySounds(true, {
    setItem: () => {
      throw new Error('blocked');
    },
  });
  expect(
    readPlaySounds({
      getItem: () => {
        throw new Error('blocked');
      },
    }),
  ).toBe(true);
});
it('is harmless without browser storage or an audio implementation', () => {
  vi.stubGlobal('localStorage', undefined);
  vi.stubGlobal('AudioContext', undefined);
  try {
    expect(() => unlockPlayAudio()).not.toThrow();
    expect(() => playSounds(['move', 'roll', 'hit', 'bear_off'])).not.toThrow();
  } finally {
    vi.unstubAllGlobals();
  }
});

it('uses the supplied cube recording for doubling alerts', () => {
  expect((playSoundFiles as Record<string, string>).cube).toMatch(
    /cube-double\.wav$/,
  );
});

it('keeps sound preference persistence separate from WAV assets and Web Audio', () => {
  const source = readFileSync(
    new URL('./play-sound-preferences.ts', import.meta.url),
    'utf8',
  );
  expect(source).not.toMatch(/\.wav|AudioContext|play-audio/);
  const events: boolean[] = [];
  const unsubscribe = onPlaySoundsChanged((enabled) => events.push(enabled));
  try {
    const values = new Map<string, string>();
    const storage = {
      setItem: (key: string, value: string) => values.set(key, value),
    };
    savePlaySounds(false, storage);
    savePlaySounds(true, storage);
    expect(events).toEqual([false, true]);
  } finally {
    unsubscribe();
  }
});
