import {
  configurationApi,
  ConfigurationConflict,
  type ConfigurationApi,
  type ConfigurationProfile,
  type ConfigurationState,
  type ConfigurationIssue,
} from './configuration-api';
import { HttpError, isConflict } from '../../shared/http/api';
import {
  isConfigurationDirty,
  normalizeConfiguration,
  type ProfileConfiguration,
} from './configuration-model';
import { restoreConfigurationField } from '../profiles/profile-changes';
import {
  configurationJournalKey,
  readConfigurationJournalRaw,
  recoverConfigurationJournal,
  writeConfigurationJournal,
  type StorageAccess,
} from './configuration-journal';
import { createProfileOperations } from './configuration-profile-ops';
import {
  applyConfigurationWithRebase,
  mutateConfigurationWithReconciliation,
  writeConfigurationWithReconciliation,
  type PendingConflict,
} from './configuration-reconcile';

export { configurationJournalKey };

export type ConfigurationStatus =
  | 'loading'
  | 'ready'
  | 'saving'
  | 'error'
  | 'conflict';
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
  private readonly profileOps: ReturnType<typeof createProfileOperations>;

  createProfile: (name: string) => Promise<void>;
  overwriteProfile: (profile: ConfigurationProfile) => Promise<void>;
  renameProfile: (profile: ConfigurationProfile, name: string) => Promise<void>;
  deleteProfile: (profile: ConfigurationProfile) => Promise<void>;
  activateProfile: (
    profile: ConfigurationProfile,
    sourceAction: 'keep' | 'save_current',
  ) => Promise<void>;
  restoreProfile: (
    profile?: Pick<ConfigurationProfile, 'id' | 'revision'> | null,
  ) => Promise<void>;
  saveConflictAsNewProfile: (name: string) => Promise<void>;

  constructor(
    private api: ConfigurationApi = configurationApi,
    private storage?: StorageAccess,
  ) {
    this.profileOps = createProfileOperations({
      api: this.api,
      getConfirmed: () => this.snapshot.confirmed,
      getCurrent: () => this.snapshot.current,
      getActiveProfile: () => this.snapshot.activeProfile,
      getLocalGeneration: () => this.localGeneration,
      requireActiveProfile: (profile) => this.requireActiveProfile(profile),
      flushNow: () => this.flushNow(),
      mutate: (path, method, body, reset) =>
        this.mutate(path, method, body, reset),
      accept: (state, generation, reset) =>
        this.accept(state, generation, reset),
      refreshAuxiliary: async () => {
        await this.loadProfiles();
        await this.loadAvailability();
      },
      operation: (fn) => this.operation(fn),
    });
    this.createProfile = this.profileOps.createProfile;
    this.overwriteProfile = this.profileOps.overwriteProfile;
    this.renameProfile = this.profileOps.renameProfile;
    this.deleteProfile = this.profileOps.deleteProfile;
    this.activateProfile = this.profileOps.activateProfile;
    this.restoreProfile = this.profileOps.restoreProfile;
    this.saveConflictAsNewProfile = this.profileOps.saveConflictAsNewProfile;
  }

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

  private journal() {
    if (!this.snapshot.confirmed) return;
    const clear =
      this.localGeneration === this.acceptedGeneration && !this.deferredDesired;
    const entry = clear
      ? null
      : this.snapshot.current
        ? {
            baseRevision:
              this.deferredBaseRevision ??
              this.recoveryBaseRevision ??
              this.snapshot.confirmed.revision,
            configuration: this.deferredDesired ?? this.snapshot.current,
          }
        : undefined;
    if (entry === undefined) return;
    if (!writeConfigurationJournal(this.storage, entry))
      this.publish({ storageError: true });
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
        (isConflict(error) &&
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

  private writeCurrent(
    revision: number,
    configuration: ProfileConfiguration,
  ) {
    return writeConfigurationWithReconciliation(
      this.api,
      revision,
      configuration,
    );
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
            error instanceof HttpError &&
            error.code === 'configuration_not_initialized'
          )
            state = await this.api.initialize();
          else throw error;
        }
        const readResult = readConfigurationJournalRaw(this.storage);
        if (!readResult.ok) this.publish({ storageError: true });
        this.accept(state, this.localGeneration, true);
        await recoverConfigurationJournal({
          raw: readResult.ok ? readResult.raw : undefined,
          serverRevision: state.revision,
          serverConfiguration: state.current_configuration,
          onRecovered: (configuration) => {
            this.localGeneration++;
            this.publish({ current: configuration });
          },
          onConflict: (baseRevision, error) => {
            this.recoveryBaseRevision = baseRevision;
            this.fail(error);
          },
          onFlush: () => this.flushNow(),
          onClean: () => this.journal(),
          onStorageError: () => this.publish({ storageError: true }),
        });
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
    this.operation(() =>
      applyConfigurationWithRebase({
        api: this.api,
        updater,
        rebaseOnConflict,
        flushNow: () => this.flushNow(),
        getState: () => ({
          confirmed: this.snapshot.confirmed,
          current: this.snapshot.current,
          status: this.snapshot.status,
          hasUnsyncedEdits: this.localGeneration !== this.acceptedGeneration,
        }),
        onSaving: () => this.publish({ status: 'saving', error: null }),
        onAccept: (state, reset, pending) => {
          if (!pending) this.localGeneration++;
          this.accept(
            state,
            pending ? this.acceptedGeneration : this.localGeneration,
            reset,
            pending,
          );
        },
        onRefreshProfiles: () => this.loadProfiles(),
        onRefreshAvailability: () => this.loadAvailability(),
      }),
    );

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
      throw new ConfigurationConflict(
        'stale_profile',
        'The active profile changed. Review its current settings before resetting.',
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
    const state = await mutateConfigurationWithReconciliation(
      this.api,
      this.snapshot.confirmed?.revision ?? 0,
      path,
      method,
      body,
    );
    this.accept(state, generation, reset);
    // A failed list refresh cannot undo an already committed operation.
    try {
      await this.loadProfiles();
    } catch {
      /* Existing list remains available for retry. */
    }
    await this.loadAvailability();
  }

  refresh = () => {
    if (!this.snapshot.ready) return this.load();
    return this.enqueue(async () => {
      try {
        const state = await this.api.read();
        if (state.revision !== (this.snapshot.confirmed?.revision ?? 0)) {
          if (this.localGeneration !== this.acceptedGeneration)
            throw new ConfigurationConflict(
              'remote_conflict',
              'Settings changed in another window',
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

  dispose = () => {
    this.cancelTimer();
    this.journal();
  };
}
