import {
  configurationApi,
  ConfigurationApiError,
  type ConfigurationApi,
  type ConfigurationProfile,
  type ConfigurationState,
  type ConfigurationIssue,
} from './configuration-api';
import {
  isConfigurationDirty,
  normalizeConfiguration,
  type ProfileConfiguration,
} from './configuration-model';
import { restoreConfigurationField } from './profile-changes';

export const configurationJournalKey = 'bgtutor.configuration-pending.v1';
type StorageAccess = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
type PendingConflict = { configuration: ProfileConfiguration; error: unknown };
export type ConfigurationStatus =
  'loading' | 'ready' | 'saving' | 'error' | 'conflict';
export interface ConfigurationSnapshot {
  confirmed: ConfigurationState | null;
  current: ProfileConfiguration | null;
  activeProfile: ConfigurationProfile | null;
  profiles: ConfigurationProfile[];
  generation: number;
  ready: boolean;
  dirty: boolean;
  busy: boolean;
  status: ConfigurationStatus;
  error: unknown;
  storageError: boolean;
  initializationWarnings: ConfigurationIssue[];
  availabilityIssues: ConfigurationIssue[];
}
export class ConfigurationStore {
  private snapshot: ConfigurationSnapshot = {
    confirmed: null,
    current: null,
    activeProfile: null,
    profiles: [],
    generation: 0,
    ready: false,
    dirty: false,
    busy: false,
    status: 'loading',
    error: null,
    storageError: false,
    initializationWarnings: [],
    availabilityIssues: [],
  };
  private listeners = new Set<() => void>();
  private queue: Promise<unknown> = Promise.resolve();
  private timer: ReturnType<typeof setTimeout> | undefined;
  private localGeneration = 0;
  private acceptedGeneration = 0;
  private recoveryBaseRevision: number | undefined;
  private deferredUpdaters: ((
    current: ProfileConfiguration,
  ) => ProfileConfiguration)[] = [];
  // A serializable recovery snapshot for edits accepted while an operation
  // owns the visible configuration. The original basis remains fixed until
  // the functional edits have been replayed against the operation's result.
  private deferredDesired: ProfileConfiguration | undefined;
  private deferredBaseRevision: number | undefined;
  private loading: Promise<void> | undefined;
  constructor(
    private api: ConfigurationApi = configurationApi,
    private storage?: StorageAccess,
  ) {}
  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  };
  getSnapshot = () => this.snapshot;
  private publish(patch: Partial<ConfigurationSnapshot>) {
    this.snapshot = { ...this.snapshot, ...patch };
    if (this.snapshot.current && this.snapshot.activeProfile)
      this.snapshot.dirty = isConfigurationDirty(
        this.snapshot.activeProfile.configuration,
        this.snapshot.current,
      );
    this.listeners.forEach((fn) => fn());
  }
  private getStorage() {
    if (this.storage) return this.storage;
    try {
      return globalThis.localStorage;
    } catch {
      return undefined;
    }
  }
  private journal() {
    if (!this.snapshot.confirmed) return;
    try {
      const storage = this.getStorage();
      if (!storage) return;
      if (
        this.localGeneration === this.acceptedGeneration &&
        !this.deferredDesired
      )
        storage.removeItem(configurationJournalKey);
      else if (this.snapshot.confirmed && this.snapshot.current)
        storage.setItem(
          configurationJournalKey,
          JSON.stringify({
            base_revision:
              this.deferredBaseRevision ??
              this.recoveryBaseRevision ??
              this.snapshot.confirmed.revision,
            configuration: this.deferredDesired ?? this.snapshot.current,
          }),
        );
    } catch {
      this.publish({ storageError: true });
    }
  }
  private accept(
    state: ConfigurationState,
    generation: number,
    reset = false,
    pending?: PendingConflict,
  ) {
    if (
      this.snapshot.confirmed &&
      state.revision < this.snapshot.confirmed.revision
    )
      return;
    const noLaterEdits = generation === this.localGeneration;
    this.recoveryBaseRevision = pending
      ? (this.recoveryBaseRevision ?? this.snapshot.confirmed?.revision)
      : undefined;
    if (noLaterEdits) this.acceptedGeneration = generation;
    this.publish({
      confirmed: state,
      activeProfile: state.active_profile,
      ...(pending
        ? { current: pending.configuration }
        : noLaterEdits
          ? { current: state.current_configuration }
          : {}),
      ready: true,
      status: pending ? 'conflict' : 'ready',
      error: pending?.error ?? null,
      generation: this.snapshot.generation + (reset ? 1 : 0),
      initializationWarnings:
        state.initialization_warnings ?? this.snapshot.initializationWarnings,
    });
    this.journal();
  }
  private fail(error: unknown) {
    this.publish({
      error,
      status:
        this.recoveryBaseRevision !== undefined ||
        this.snapshot.status === 'conflict' ||
        (error instanceof ConfigurationApiError &&
          error.status === 409 &&
          !['duplicate_name', 'active_profile'].includes(error.code))
          ? 'conflict'
          : 'error',
    });
    this.journal();
  }
  private enqueue<T>(fn: () => Promise<T>): Promise<T> {
    const result = this.queue.then(fn, fn);
    this.queue = result.catch(() => {});
    return result;
  }
  private cancelTimer() {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = undefined;
    }
  }
  private async loadProfiles() {
    const profiles = await this.api.listProfiles();
    this.publish({
      profiles: profiles.sort((a, b) => a.name.localeCompare(b.name)),
    });
  }
  private async loadAvailability() {
    if (!this.api.availability) return;
    try {
      const report = await this.api.availability();
      if (report.revision === this.snapshot.confirmed?.revision)
        this.publish({ availabilityIssues: report.issues });
    } catch {
      /* A transient availability check does not discard the saved configuration. */
    }
  }
  private replayDeferred() {
    const pending = this.deferredUpdaters;
    this.deferredUpdaters = [];
    for (const update of pending) this.updateCurrent(update);
    // Keep the recovery snapshot until all callbacks have been applied. Each
    // callback may journal, but only this final write replaces the old basis
    // with the resulting configuration and its confirmed server revision.
    this.deferredDesired = undefined;
    this.deferredBaseRevision = undefined;
    this.journal();
  }
  private async writeCurrent(
    revision: number,
    configuration: ProfileConfiguration,
  ) {
    try {
      return await this.api.write(revision, configuration);
    } catch (error) {
      if (error instanceof ConfigurationApiError) throw error;
      try {
        const remote = await this.api.read();
        if (remote.revision > revision) {
          if (
            !isConfigurationDirty(remote.current_configuration, configuration)
          )
            return remote;
          throw new ConfigurationApiError(
            409,
            'Settings changed after an interrupted save',
            'uncertain_result',
          );
        }
      } catch (reconciliation) {
        if (reconciliation instanceof ConfigurationApiError)
          throw reconciliation;
      }
      throw error;
    }
  }
  load = (): Promise<void> => {
    if (this.loading) return this.loading;
    this.loading = this.enqueue(async () => {
      try {
        let state: ConfigurationState;
        try {
          state = await this.api.read();
        } catch (error) {
          if (
            error instanceof ConfigurationApiError &&
            error.code === 'configuration_not_initialized'
          )
            state = await this.api.initialize();
          else throw error;
        }
        let rawJournal: string | null | undefined;
        try {
          rawJournal = this.getStorage()?.getItem(configurationJournalKey);
        } catch {
          this.publish({ storageError: true });
        }
        this.accept(state, this.localGeneration, true);
        try {
          const raw = rawJournal;
          if (raw) {
            const saved = JSON.parse(raw);
            const pending = normalizeConfiguration(saved.configuration);
            if (isConfigurationDirty(state.current_configuration, pending)) {
              this.localGeneration++;
              this.publish({ current: pending });
              if (saved.base_revision !== state.revision) {
                this.recoveryBaseRevision = saved.base_revision;
                this.fail(
                  new ConfigurationApiError(
                    409,
                    'Saved local changes conflict with newer settings',
                    'journal_conflict',
                  ),
                );
              } else await this.flushNow();
            } else this.journal();
          }
        } catch (error) {
          if (error instanceof ConfigurationApiError) throw error;
          this.publish({ storageError: true });
        }
        await this.loadProfiles();
        await this.loadAvailability();
        this.replayDeferred();
      } catch (error) {
        this.fail(error);
      }
    }).finally(() => {
      this.loading = undefined;
    });
    return this.loading;
  };
  updateCurrent = (
    updater: (current: ProfileConfiguration) => ProfileConfiguration,
  ) => {
    if (!this.snapshot.current || this.snapshot.busy) {
      if (this.snapshot.current) {
        this.deferredDesired = normalizeConfiguration(
          updater(
            structuredClone(this.deferredDesired ?? this.snapshot.current),
          ),
        );
        this.deferredBaseRevision ??=
          this.recoveryBaseRevision ?? this.snapshot.confirmed?.revision;
      }
      this.deferredUpdaters.push(updater);
      this.journal();
      return;
    }
    const current = normalizeConfiguration(
      updater(structuredClone(this.snapshot.current)),
    );
    if (!isConfigurationDirty(this.snapshot.current, current)) return;
    this.localGeneration++;
    this.publish({ current });
    this.journal();
    this.cancelTimer();
    if (this.snapshot.status !== 'conflict')
      this.timer = setTimeout(() => {
        void this.flush().catch(() => {});
      }, 300);
  };
  private async flushNow() {
    this.cancelTimer();
    if (this.snapshot.status === 'conflict') throw this.snapshot.error;
    if (!this.snapshot.confirmed || !this.snapshot.current)
      throw new Error('Configuration is not loaded');
    while (this.localGeneration !== this.acceptedGeneration) {
      const generation = this.localGeneration;
      this.publish({ status: 'saving', error: null });
      try {
        const state = await this.writeCurrent(
          this.snapshot.confirmed.revision,
          this.snapshot.current,
        );
        this.accept(state, generation);
      } catch (error) {
        this.fail(error);
        throw error;
      }
    }
    if (this.snapshot.status === 'error')
      this.publish({ status: 'ready', error: null });
  }
  flush = () => this.enqueue(() => this.flushNow());
  applyCurrent = (
    updater: (current: ProfileConfiguration) => ProfileConfiguration,
    { rebaseOnConflict = false } = {},
  ) =>
    this.operation(async () => {
      let pending: PendingConflict | undefined;
      let base = this.snapshot.confirmed;
      try {
        await this.flushNow();
        base = this.snapshot.confirmed;
      } catch (error) {
        if (!rebaseOnConflict || this.snapshot.status !== 'conflict')
          throw error;
        base = await this.api.read();
        if (this.localGeneration !== this.acceptedGeneration)
          pending = { configuration: this.snapshot.current!, error };
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
        const proposed = normalizeConfiguration(
          updater(structuredClone(current)),
        );
        let state = base;
        try {
          if (isConfigurationDirty(current, proposed)) {
            if (!pending) this.publish({ status: 'saving', error: null });
            state = await this.writeCurrent(base.revision, proposed);
          }
        } catch (error) {
          if (
            !canRetry ||
            !(error instanceof ConfigurationApiError) ||
            error.code !== 'revision_conflict'
          )
            throw error;
          const remote = await this.api.read();
          if (remote.revision <= base.revision) throw error;
          base = remote;
          continue;
        }
        const confirmed = this.snapshot.confirmed;
        if (state === confirmed && this.snapshot.status === 'ready')
          return current;
        if (
          pending &&
          !isConfigurationDirty(
            state.current_configuration,
            pending.configuration,
          )
        )
          pending = undefined;
        if (!pending) this.localGeneration++;
        this.accept(
          state,
          pending ? this.acceptedGeneration : this.localGeneration,
          confirmed?.revision !== base.revision,
          pending,
        );
        if (state.active_profile.id !== confirmed?.active_profile.id) {
          try {
            await this.loadProfiles();
          } catch {
            /* Keep the previous list until the next refresh. */
          }
        }
        await this.loadAvailability();
        return state.current_configuration;
      }
    });
  restoreProfileField = (
    profile: Pick<ConfigurationProfile, 'id' | 'revision'>,
    path: string,
  ) =>
    this.applyCurrent((current) => {
      const active = this.requireActiveProfile(profile);
      return restoreConfigurationField(active.configuration, current, path);
    });
  private requireActiveProfile(
    profile: Pick<ConfigurationProfile, 'id' | 'revision'> | null,
  ) {
    const active = this.snapshot.activeProfile;
    if (
      !profile ||
      !active ||
      active.id !== profile.id ||
      active.revision !== profile.revision
    )
      throw new ConfigurationApiError(
        409,
        'The active profile changed. Review its current settings before resetting.',
        'stale_profile',
      );
    return active;
  }
  private operation<T>(fn: () => Promise<T>): Promise<T> {
    if (this.snapshot.busy)
      return Promise.reject(
        new Error('A configuration operation is already running'),
      );
    this.cancelTimer();
    this.publish({ busy: true });
    return this.enqueue(async () => {
      try {
        return await fn();
      } catch (error) {
        this.fail(error);
        throw error;
      } finally {
        this.publish({ busy: false });
        this.replayDeferred();
      }
    });
  }
  private async mutate(
    path: string,
    method: string,
    body: unknown,
    reset = false,
  ) {
    const generation = this.localGeneration;
    this.publish({ status: 'saving', error: null });
    try {
      const state = await this.api.mutate(path, method, body);
      this.accept(state, generation, reset);
      // A failed list refresh cannot undo an already committed operation.
      try {
        await this.loadProfiles();
      } catch {
        /* Existing list remains available for retry. */
      }
      await this.loadAvailability();
    } catch (error) {
      if (!(error instanceof ConfigurationApiError)) {
        try {
          const remote = await this.api.read();
          if (remote.revision !== (this.snapshot.confirmed?.revision ?? 0))
            throw new ConfigurationApiError(
              409,
              'The server changed after an interrupted request. Reload the server state to reconcile.',
              'uncertain_result',
            );
        } catch (reconciliation) {
          if (reconciliation instanceof ConfigurationApiError)
            throw reconciliation;
        }
      }
      throw error;
    }
  }
  createProfile = (name: string) =>
    this.operation(async () => {
      await this.flushNow();
      await this.mutate(
        '/profiles',
        'POST',
        {
          name,
          expected_state_revision: this.snapshot.confirmed!.revision,
          configuration: this.snapshot.current,
        },
        true,
      );
    });
  overwriteProfile = (profile: ConfigurationProfile) =>
    this.operation(async () => {
      await this.flushNow();
      await this.mutate(
        `/profiles/${encodeURIComponent(profile.id)}/snapshot`,
        'PUT',
        {
          expected_state_revision: this.snapshot.confirmed!.revision,
          expected_profile_revision: profile.revision,
          configuration: this.snapshot.current,
        },
      );
    });
  renameProfile = (profile: ConfigurationProfile, name: string) =>
    this.operation(async () => {
      await this.flushNow();
      await this.mutate(
        `/profiles/${encodeURIComponent(profile.id)}`,
        'PATCH',
        {
          name,
          expected_state_revision: this.snapshot.confirmed!.revision,
          expected_profile_revision: profile.revision,
        },
      );
    });
  deleteProfile = (profile: ConfigurationProfile) =>
    this.operation(async () => {
      await this.flushNow();
      await this.mutate(
        `/profiles/${encodeURIComponent(profile.id)}`,
        'DELETE',
        {
          expected_state_revision: this.snapshot.confirmed!.revision,
          expected_profile_revision: profile.revision,
        },
      );
    });
  activateProfile = (
    profile: ConfigurationProfile,
    sourceAction: 'keep' | 'save_current',
  ) =>
    this.operation(async () => {
      await this.flushNow();
      if (profile.id === this.snapshot.activeProfile?.id) return;
      await this.mutate(
        `/profiles/${encodeURIComponent(profile.id)}/activate`,
        'POST',
        {
          expected_state_revision: this.snapshot.confirmed!.revision,
          expected_target_revision: profile.revision,
          expected_source_revision: this.snapshot.activeProfile!.revision,
          source_action: sourceAction,
          current_configuration: this.snapshot.current,
        },
        true,
      );
    });
  restoreProfile = (
    profile: Pick<ConfigurationProfile, 'id' | 'revision'> | null = this
      .snapshot.activeProfile,
  ) =>
    this.operation(async () => {
      this.requireActiveProfile(profile);
      await this.flushNow();
      this.requireActiveProfile(profile);
      await this.mutate(
        '/configuration/restore-profile',
        'POST',
        {
          expected_state_revision: this.snapshot.confirmed!.revision,
          expected_profile_revision: this.snapshot.activeProfile!.revision,
        },
        true,
      );
    });
  runWithConfiguration = <T>(
    work: (current: ProfileConfiguration) => Promise<T>,
  ): Promise<T> =>
    this.operation(async () => {
      await this.flushNow();
      const engineIssues = this.snapshot.availabilityIssues.filter(
        (issue) =>
          issue.path.includes('engine') || issue.path.endsWith('.plies'),
      );
      if (engineIssues.length)
        throw new ConfigurationApiError(
          422,
          'Configured engine is unavailable',
          'unavailable_resources',
          engineIssues,
        );
      return work(structuredClone(this.snapshot.current!));
    });
  refresh = () => {
    if (!this.snapshot.ready) return this.load();
    return this.enqueue(async () => {
      try {
        const state = await this.api.read();
        if (state.revision !== (this.snapshot.confirmed?.revision ?? 0)) {
          if (this.localGeneration !== this.acceptedGeneration)
            throw new ConfigurationApiError(
              409,
              'Settings changed in another window',
              'remote_conflict',
            );
          this.accept(state, this.localGeneration, true);
        }
        await this.loadProfiles();
        await this.loadAvailability();
        if (
          this.snapshot.status === 'error' &&
          this.localGeneration === this.acceptedGeneration &&
          !this.deferredDesired
        )
          this.publish({ status: 'ready', error: null });
      } catch (error) {
        this.fail(error);
      }
    });
  };
  reloadServer = () =>
    this.operation(async () => {
      const state = await this.api.read();
      this.localGeneration++;
      this.acceptedGeneration = this.localGeneration;
      this.accept(state, this.localGeneration, true);
      await this.loadProfiles();
      await this.loadAvailability();
    });
  saveConflictAsNewProfile = (name: string) =>
    this.operation(async () => {
      if (!this.snapshot.current)
        throw new Error('Configuration is not loaded');
      const local = this.snapshot.current;
      const remote = await this.api.read();
      const state = await this.api.mutate('/profiles', 'POST', {
        name,
        expected_state_revision: remote.revision,
        configuration: local,
      });
      this.accept(state, this.localGeneration, true);
      await this.loadProfiles();
      await this.loadAvailability();
    });
  dispose = () => {
    this.cancelTimer();
    this.journal();
  };
}
