import { isConflict } from '../../shared/http/api';
import {
  ConfigurationConflict,
  isDecided,
  type ConfigurationApi,
  type ConfigurationState,
} from './configuration-api';
import {
  isConfigurationDirty,
  normalizeConfiguration,
  type ProfileConfiguration,
} from './configuration-model';

export type PendingConflict = {
  configuration: ProfileConfiguration;
  error: unknown;
};

/**
 * Writes the configuration and reconciles ambiguous network errors by reading
 * the server state back to detect whether the write succeeded or conflicted.
 */
export async function writeConfigurationWithReconciliation(
  api: ConfigurationApi,
  revision: number,
  configuration: ProfileConfiguration,
): Promise<ConfigurationState> {
  try {
    return await api.write(revision, configuration);
  } catch (error) {
    if (isDecided(error)) throw error;
    try {
      const remote = await api.read();
      if (remote.revision > revision) {
        if (!isConfigurationDirty(remote.current_configuration, configuration))
          return remote;
        throw new ConfigurationConflict(
          'uncertain_result',
          'Settings changed after an interrupted save',
        );
      }
    } catch (reconciliation) {
      if (isDecided(reconciliation)) throw reconciliation;
    }
    throw error;
  }
}

/**
 * Executes a profile mutation and reconciles ambiguous transport errors by
 * checking whether the server revision moved ahead.
 */
export async function mutateConfigurationWithReconciliation(
  api: ConfigurationApi,
  confirmedRevision: number,
  path: string,
  method: string,
  body: unknown,
): Promise<ConfigurationState> {
  try {
    return await api.mutate(path, method, body);
  } catch (error) {
    if (!isDecided(error)) {
      try {
        const remote = await api.read();
        if (remote.revision !== confirmedRevision)
          throw new ConfigurationConflict(
            'uncertain_result',
            'The server changed after an interrupted request. Reload the server state to reconcile.',
          );
      } catch (reconciliation) {
        if (isDecided(reconciliation)) throw reconciliation;
      }
    }
    throw error;
  }
}

export async function applyConfigurationWithRebase({
  api,
  updater,
  rebaseOnConflict,
  flushNow,
  getState,
  onSaving,
  onAccept,
  onRefreshProfiles,
  onRefreshAvailability,
}: {
  api: ConfigurationApi;
  updater: (current: ProfileConfiguration) => ProfileConfiguration;
  rebaseOnConflict: boolean;
  flushNow: () => Promise<void>;
  getState: () => {
    confirmed: ConfigurationState | null;
    current: ProfileConfiguration | null;
    status: string;
    hasUnsyncedEdits: boolean;
  };
  onSaving: () => void;
  onAccept: (
    state: ConfigurationState,
    reset: boolean,
    pending?: PendingConflict,
  ) => void;
  onRefreshProfiles: () => Promise<void>;
  onRefreshAvailability: () => Promise<void>;
}): Promise<ProfileConfiguration> {
  let pending: PendingConflict | undefined;
  let base = getState().confirmed;
  try {
    await flushNow();
    base = getState().confirmed;
  } catch (error) {
    const state = getState();
    if (!rebaseOnConflict || state.status !== 'conflict') throw error;
    base = await api.read();
    if (state.hasUnsyncedEdits)
      pending = { configuration: state.current!, error };
  }
  if (!base) throw new Error('Configuration is not loaded');

  // An explicit settings draft can be saved while unrelated autosaved edits
  // still need conflict recovery. Apply the draft to both versions separately.
  if (pending)
    pending.configuration = normalizeConfiguration(
      updater(structuredClone(pending.configuration)),
    );
  for (let canRetry = rebaseOnConflict; ; canRetry = false) {
    const current = base.current_configuration;
    const proposed = normalizeConfiguration(updater(structuredClone(current)));
    let state = base;
    try {
      if (isConfigurationDirty(current, proposed)) {
        if (!pending) onSaving();
        state = await writeConfigurationWithReconciliation(
          api,
          base.revision,
          proposed,
        );
      }
    } catch (error) {
      if (
        !canRetry ||
        !isConflict(error) ||
        error.code !== 'revision_conflict'
      )
        throw error;
      const remote = await api.read();
      if (remote.revision <= base.revision) throw error;
      base = remote;
      continue;
    }
    const { confirmed, status } = getState();
    if (state === confirmed && status === 'ready') return current;
    if (
      pending &&
      !isConfigurationDirty(state.current_configuration, pending.configuration)
    )
      pending = undefined;
    onAccept(state, confirmed?.revision !== base.revision, pending);
    if (state.active_profile.id !== confirmed?.active_profile.id) {
      try {
        await onRefreshProfiles();
      } catch {
        /* Keep the previous list until the next refresh. */
      }
    }
    await onRefreshAvailability();
    return state.current_configuration;
  }
}
