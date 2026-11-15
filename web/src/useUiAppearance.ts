import { useConfirmedConfiguration } from './ConfigurationProvider';
import { useLayoutEffect, useState, useSyncExternalStore } from 'react';
import {
  applyUiAppearance,
  observeSystemAppearance,
  readUiAppearance,
  resolveUiMode,
  saveUiAppearance,
  systemIsDark,
  type UiAppearance,
  type UiMode,
} from './ui-appearance';

const noSubscription = () => () => {};
const serverDark = () => false;
const subscribe = (notify: () => void) =>
  typeof window.matchMedia === 'function'
    ? observeSystemAppearance(notify)
    : noSubscription();

export function useResolvedUiMode(mode: UiMode) {
  const dark = useSyncExternalStore(
    mode === 'system' ? subscribe : noSubscription,
    systemIsDark,
    serverDark,
  );
  return resolveUiMode(mode, dark);
}

export function useUiAppearance() {
  const confirmed = useConfirmedConfiguration();
  const [localAppearance, setAppearance] = useState(readUiAppearance);
  const appearance = confirmed
    ? {
        theme: confirmed.appearance.ui_theme,
        mode: confirmed.appearance.mode,
      }
    : localAppearance;
  const resolvedMode = useResolvedUiMode(appearance.mode);
  useLayoutEffect(() => {
    applyUiAppearance(appearance, resolvedMode === 'dark');
  }, [appearance, resolvedMode]);
  function applyAppearance(value: UiAppearance) {
    saveUiAppearance(value);
    setAppearance(value);
  }
  return { appearance, applyAppearance };
}
