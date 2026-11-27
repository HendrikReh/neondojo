export const playSoundsStorageKey = 'neondojo.play-sounds';
let enabledInMemory = true;
const listeners = new Set<(enabled: boolean) => void>();

export function onPlaySoundsChanged(
  listener: (enabled: boolean) => void,
): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

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
  for (const listener of listeners) listener(enabled);
}
