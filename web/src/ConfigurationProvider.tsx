import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from 'react';
import {
  ConfigurationStore,
  type ConfigurationSnapshot,
} from './configuration-store';
import { cacheConfiguration } from './configuration-legacy';

export type ConfigurationContextValue = ConfigurationSnapshot &
  Pick<
    ConfigurationStore,
    | 'updateCurrent'
    | 'applyCurrent'
    | 'flush'
    | 'refresh'
    | 'createProfile'
    | 'overwriteProfile'
    | 'renameProfile'
    | 'deleteProfile'
    | 'activateProfile'
    | 'restoreProfile'
    | 'restoreProfileField'
    | 'reloadServer'
    | 'saveConflictAsNewProfile'
    | 'runWithConfiguration'
  >;
const ConfigurationContext = createContext<ConfigurationContextValue | null>(
  null,
);
export function ConfigurationProvider({
  children,
  store: provided,
}: {
  children: ReactNode;
  store?: ConfigurationStore;
}) {
  const [store] = useState(() => provided ?? new ConfigurationStore());
  const state = useSyncExternalStore(
    store.subscribe,
    store.getSnapshot,
    store.getSnapshot,
  );
  useEffect(() => {
    void store.load();
    const focus = () => {
      void store.refresh();
    };
    window.addEventListener('focus', focus);
    window.addEventListener('pagehide', store.dispose);
    const channel =
      typeof BroadcastChannel === 'function'
        ? new BroadcastChannel('bgtutor.configuration')
        : null;
    if (channel) channel.onmessage = focus;
    return () => {
      window.removeEventListener('focus', focus);
      window.removeEventListener('pagehide', store.dispose);
      channel?.close();
      store.dispose();
    };
  }, [store]);
  useEffect(() => {
    if (state.current) cacheConfiguration(state.current);
  }, [state.current]);
  useEffect(() => {
    if (!state.confirmed) return;
    const channel =
      typeof BroadcastChannel === 'function'
        ? new BroadcastChannel('bgtutor.configuration')
        : null;
    channel?.postMessage({ revision: state.confirmed.revision });
    channel?.close();
  }, [state.confirmed]);
  const value = useMemo(
    () => ({
      ...state,
      updateCurrent: store.updateCurrent,
      applyCurrent: store.applyCurrent,
      flush: store.flush,
      refresh: store.refresh,
      createProfile: store.createProfile,
      overwriteProfile: store.overwriteProfile,
      renameProfile: store.renameProfile,
      deleteProfile: store.deleteProfile,
      activateProfile: store.activateProfile,
      restoreProfile: store.restoreProfile,
      restoreProfileField: store.restoreProfileField,
      reloadServer: store.reloadServer,
      saveConflictAsNewProfile: store.saveConflictAsNewProfile,
      runWithConfiguration: store.runWithConfiguration,
    }),
    [state, store],
  );
  return (
    <ConfigurationContext.Provider value={value}>
      {children}
    </ConfigurationContext.Provider>
  );
}
export function useOptionalConfiguration() {
  return useContext(ConfigurationContext);
}
export function useConfiguration() {
  const value = useOptionalConfiguration();
  if (!value) throw new Error('ConfigurationProvider is required');
  return value;
}
