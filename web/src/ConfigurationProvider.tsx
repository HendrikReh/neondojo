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
import {
  defaultConfiguration,
  normalizeConfiguration,
  type ProfileConfiguration,
} from './configuration-model';

export type ConfigurationContextValue = ConfigurationSnapshot &
  Pick<
    ConfigurationStore,
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
  >;
const ConfigurationContext = createContext<ConfigurationContextValue | null>(
  null,
);
const ConfirmedConfigurationContext = createContext<
  ProfileConfiguration | undefined
>(undefined);
const WorkspaceDefaultsContext = createContext<
  ProfileConfiguration | undefined
>(undefined);
const confirmedKey = 'bgtutor.configuration-confirmed.v1';
const workspaceDefaultsKey = 'bgtutor.workspace-defaults.v1';

function readCachedConfiguration(key: string) {
  try {
    const value = localStorage.getItem(key);
    return value ? normalizeConfiguration(JSON.parse(value)) : undefined;
  } catch {
    return undefined;
  }
}

function writeCachedConfiguration(key: string, value: ProfileConfiguration) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // The current tab can still use the in-memory settings.
  }
}

/** Read-only settings for workspaces. Drafts and profile errors stay in management. */
export function useConfirmedConfiguration() {
  return useContext(ConfirmedConfigurationContext);
}

/** One-time seed from older installations, shared across tabs in this browser. */
export function useWorkspaceDefaults() {
  return useContext(WorkspaceDefaultsContext);
}
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
  const [cached] = useState(() => readCachedConfiguration(confirmedKey));
  const [workspaceDefaults, setWorkspaceDefaults] = useState(() =>
    readCachedConfiguration(workspaceDefaultsKey),
  );
  const confirmed = state.confirmed?.current_configuration;
  const effective = confirmed ?? cached;
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
    if (!confirmed) return;
    cacheConfiguration(confirmed);
    writeCachedConfiguration(confirmedKey, confirmed);
  }, [confirmed]);
  useEffect(() => {
    if (workspaceDefaults || !effective) return;
    const initial = readCachedConfiguration(workspaceDefaultsKey) ?? {
      ...defaultConfiguration,
      analysis: {
        ...defaultConfiguration.analysis,
        scope: effective.analysis.scope,
      },
      appearance: effective.appearance,
      library: effective.library,
      positions: effective.positions,
      learning: effective.learning,
      annotations: effective.annotations,
    };
    setWorkspaceDefaults(initial);
    writeCachedConfiguration(workspaceDefaultsKey, initial);
  }, [effective, workspaceDefaults]);
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
    }),
    [state, store],
  );
  return (
    <ConfigurationContext.Provider value={value}>
      <ConfirmedConfigurationContext.Provider value={effective}>
        <WorkspaceDefaultsContext.Provider
          value={workspaceDefaults ?? effective}
        >
          {children}
        </WorkspaceDefaultsContext.Provider>
      </ConfirmedConfigurationContext.Provider>
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
