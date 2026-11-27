import type {
  ConfigurationApi,
  ConfigurationProfile,
  ConfigurationState,
} from './configuration-api';
import type { ProfileConfiguration } from './configuration-model';

export interface ProfileOpsHost {
  api: ConfigurationApi;
  getConfirmed: () => ConfigurationState | null;
  getCurrent: () => ProfileConfiguration | null;
  getActiveProfile: () => ConfigurationProfile | null;
  getLocalGeneration: () => number;
  requireActiveProfile: (
    profile: Pick<ConfigurationProfile, 'id' | 'revision'> | null,
  ) => ConfigurationProfile;
  flushNow: () => Promise<void>;
  mutate: (
    path: string,
    method: string,
    body: unknown,
    reset?: boolean,
  ) => Promise<void>;
  accept: (
    state: ConfigurationState,
    generation: number,
    reset?: boolean,
  ) => void;
  refreshAuxiliary: () => Promise<void>;
  operation: <T>(fn: () => Promise<T>) => Promise<T>;
}

export function createProfileOperations(host: ProfileOpsHost) {
  return {
    createProfile: (name: string) =>
      host.operation(async () => {
        await host.flushNow();
        await host.mutate(
          '/profiles',
          'POST',
          {
            name,
            expected_state_revision: host.getConfirmed()!.revision,
            configuration: host.getCurrent(),
          },
          true,
        );
      }),

    overwriteProfile: (profile: ConfigurationProfile) =>
      host.operation(async () => {
        await host.flushNow();
        await host.mutate(
          `/profiles/${encodeURIComponent(profile.id)}/snapshot`,
          'PUT',
          {
            expected_state_revision: host.getConfirmed()!.revision,
            expected_profile_revision: profile.revision,
            configuration: host.getCurrent(),
          },
        );
      }),

    renameProfile: (profile: ConfigurationProfile, name: string) =>
      host.operation(async () => {
        await host.flushNow();
        await host.mutate(
          `/profiles/${encodeURIComponent(profile.id)}`,
          'PATCH',
          {
            name,
            expected_state_revision: host.getConfirmed()!.revision,
            expected_profile_revision: profile.revision,
          },
        );
      }),

    deleteProfile: (profile: ConfigurationProfile) =>
      host.operation(async () => {
        await host.flushNow();
        await host.mutate(
          `/profiles/${encodeURIComponent(profile.id)}`,
          'DELETE',
          {
            expected_state_revision: host.getConfirmed()!.revision,
            expected_profile_revision: profile.revision,
          },
        );
      }),

    activateProfile: (
      profile: ConfigurationProfile,
      sourceAction: 'keep' | 'save_current',
    ) =>
      host.operation(async () => {
        await host.flushNow();
        const active = host.getActiveProfile();
        if (profile.id === active?.id) return;
        await host.mutate(
          `/profiles/${encodeURIComponent(profile.id)}/activate`,
          'POST',
          {
            expected_state_revision: host.getConfirmed()!.revision,
            expected_target_revision: profile.revision,
            expected_source_revision: active!.revision,
            source_action: sourceAction,
            current_configuration: host.getCurrent(),
          },
          true,
        );
      }),

    restoreProfile: (
      profile: Pick<ConfigurationProfile, 'id' | 'revision'> | null = host.getActiveProfile(),
    ) =>
      host.operation(async () => {
        host.requireActiveProfile(profile);
        await host.flushNow();
        host.requireActiveProfile(profile);
        await host.mutate(
          '/configuration/restore-profile',
          'POST',
          {
            expected_state_revision: host.getConfirmed()!.revision,
            expected_profile_revision: host.getActiveProfile()!.revision,
          },
          true,
        );
      }),

    saveConflictAsNewProfile: (name: string) =>
      host.operation(async () => {
        const local = host.getCurrent();
        if (!local) throw new Error('Configuration is not loaded');
        const remote = await host.api.read();
        const state = await host.api.mutate('/profiles', 'POST', {
          name,
          expected_state_revision: remote.revision,
          configuration: local,
        });
        host.accept(state, host.getLocalGeneration(), true);
        await host.refreshAuxiliary();
      }),
  };
}
