import { describe, expect, it, vi } from 'vitest';
import { ConfigurationStore } from './configuration-store';
import { defaultConfiguration } from './configuration-model';
import {
  ConfigurationApiError,
  type ConfigurationApi,
  type ConfigurationState,
} from './configuration-api';

function harness() {
  let state: ConfigurationState = {
    revision: 1,
    active_profile: {
      id: 'p1',
      name: 'Standard',
      revision: 1,
      configuration: structuredClone(defaultConfiguration),
    },
    current_configuration: structuredClone(defaultConfiguration),
    dirty: false,
    differences: [],
  };
  const calls: string[] = [];
  let release: (() => void) | undefined;
  let wait: Promise<void> | undefined;
  let mutationWait: Promise<void> | undefined;
  let releaseMutation: (() => void) | undefined;
  let failList = false;
  let available = true;
  const api: ConfigurationApi = {
    read: async () => structuredClone(state),
    initialize: async () => structuredClone(state),
    listProfiles: async () => {
      if (failList) {
        failList = false;
        throw new Error('Profile list unavailable');
      }
      return [state.active_profile];
    },
    availability: async () => ({
      revision: state.revision,
      valid: available,
      issues: available
        ? []
        : [
            {
              path: 'analysis.engine',
              code: 'unavailable_engine',
              message: 'Engine unavailable',
            },
          ],
    }),
    write: async (revision, configuration) => {
      calls.push('write');
      if (wait) await wait;
      if (revision !== state.revision)
        throw new ConfigurationApiError(409, 'Conflict');
      state = {
        ...state,
        revision: state.revision + 1,
        current_configuration: structuredClone(configuration),
      };
      return structuredClone(state);
    },
    mutate: async (path, _method, body) => {
      calls.push(path);
      if (mutationWait) await mutationWait;
      if (
        (body as { expected_state_revision: number })
          .expected_state_revision !== state.revision
      )
        throw new ConfigurationApiError(409, 'Conflict');
      const restoring = path === '/configuration/restore-profile';
      if (
        restoring &&
        (body as { expected_profile_revision: number })
          .expected_profile_revision !== state.active_profile.revision
      )
        throw new ConfigurationApiError(409, 'Conflict');
      state = {
        ...state,
        revision: state.revision + 1,
        active_profile: restoring
          ? state.active_profile
          : { ...state.active_profile, id: 'p2', name: 'Other' },
        current_configuration: structuredClone(
          restoring ? state.active_profile.configuration : defaultConfiguration,
        ),
      };
      return structuredClone(state);
    },
  };
  const storage = new Map<string, string>();
  const storageAccess = {
    getItem: (k: string) => storage.get(k) ?? null,
    setItem: (k: string, v: string) => {
      storage.set(k, v);
    },
    removeItem: (k: string) => {
      storage.delete(k);
    },
  };
  const store = new ConfigurationStore(api, storageAccess);
  return {
    store,
    api,
    calls,
    storage,
    setAvailable: (value: boolean) => {
      available = value;
    },
    newStore: () => new ConfigurationStore(api, storageAccess),
    failListOnce: () => {
      failList = true;
    },
    holdMutation: () => {
      mutationWait = new Promise<void>((resolve) => {
        releaseMutation = resolve;
      });
    },
    releaseMutation: () => {
      releaseMutation?.();
      mutationWait = undefined;
    },
    commitOtherProfile: () => {
      state = {
        ...state,
        revision: state.revision + 1,
        active_profile: { ...state.active_profile, id: 'p2', name: 'Other' },
      };
    },
    hold() {
      wait = new Promise<void>((r) => {
        release = r;
      });
    },
    release() {
      release?.();
      wait = undefined;
    },
    conflict() {
      state.revision++;
    },
  };
}
describe('configuration writing', () => {
  for (const kind of ['id', 'revision'] as const)
    it(`rejects a bulk restore against an outdated profile ${kind} before writing`, async () => {
      const h = harness();
      await h.store.load();
      const old = h.store.getSnapshot().activeProfile!;
      const remote = await h.api.read();
      remote.revision++;
      if (kind === 'id') remote.active_profile.id = 'p2';
      else remote.active_profile.revision++;
      vi.spyOn(h.api, 'read').mockResolvedValueOnce(remote);
      await h.store.refresh();
      await expect(h.store.restoreProfile(old)).rejects.toThrow();
      expect(h.calls).toEqual([]);
    });
  it('restores the complete saved configuration in one operation without altering the profile', async () => {
    const h = harness();
    await h.store.load();
    await h.store.applyCurrent((c) => ({
      ...c,
      analysis: { ...c.analysis, plies: 4 },
      general: { ...c.general, play_sounds: false },
    }));
    const saved = structuredClone(h.store.getSnapshot().activeProfile!);
    h.calls.length = 0;
    await h.store.restoreProfile(saved);
    expect(h.store.getSnapshot().current).toEqual(saved.configuration);
    expect(h.store.getSnapshot().activeProfile).toEqual(saved);
    expect(h.store.getSnapshot().dirty).toBe(false);
    expect(h.calls).toEqual(['/configuration/restore-profile']);
  });
  it('keeps all current values when full restoration fails resource validation', async () => {
    const h = harness();
    await h.store.load();
    await h.store.applyCurrent((c) => ({
      ...c,
      analysis: { ...c.analysis, plies: 4 },
    }));
    const before = structuredClone(h.store.getSnapshot().current);
    vi.spyOn(h.api, 'mutate').mockRejectedValueOnce(
      new ConfigurationApiError(422, 'Saved resource unavailable'),
    );
    await expect(
      h.store.restoreProfile(h.store.getSnapshot().activeProfile!),
    ).rejects.toThrow('Saved resource unavailable');
    expect(h.store.getSnapshot().current).toEqual(before);
    expect(h.store.getSnapshot().busy).toBe(false);
  });
  it('replays newer local edits after restoring the saved profile', async () => {
    const h = harness();
    await h.store.load();
    await h.store.applyCurrent((c) => ({
      ...c,
      analysis: { ...c.analysis, plies: 4 },
    }));
    h.calls.length = 0;
    h.holdMutation();
    const restoring = h.store.restoreProfile(
      h.store.getSnapshot().activeProfile!,
    );
    await vi.waitFor(() =>
      expect(h.calls).toEqual(['/configuration/restore-profile']),
    );
    h.store.updateCurrent((c) => ({
      ...c,
      general: { ...c.general, player_name: 'Newer local edit' },
    }));
    h.releaseMutation();
    await restoring;
    expect(h.store.getSnapshot().current?.analysis.plies).toBe(2);
    expect(h.store.getSnapshot().current?.general.player_name).toBe(
      'Newer local edit',
    );
    await h.store.flush();
    expect(
      h.store.getSnapshot().activeProfile?.configuration.general.player_name,
    ).toBe('');
  });
  it('atomically restores only the selected field without changing the saved profile', async () => {
    const h = harness();
    await h.store.load();
    await h.store.applyCurrent((c) => ({
      ...c,
      analysis: { ...c.analysis, plies: 4 },
      appearance: { ...c.appearance, board_theme: 'walnut' },
    }));
    const saved = structuredClone(h.store.getSnapshot().activeProfile!);
    h.calls.length = 0;
    await h.store.restoreProfileField(saved, 'analysis.plies');
    expect(h.store.getSnapshot().current?.analysis.plies).toBe(2);
    expect(h.store.getSnapshot().current?.appearance.board_theme).toBe(
      'walnut',
    );
    expect(h.store.getSnapshot().activeProfile).toEqual(saved);
    expect(h.store.getSnapshot().dirty).toBe(true);
    expect(h.calls).toEqual(['write']);
  });
  it('retains the previous field value if a restored resource is rejected', async () => {
    const h = harness();
    await h.store.load();
    await h.store.applyCurrent((c) => ({
      ...c,
      analysis: { ...c.analysis, plies: 4 },
    }));
    const before = structuredClone(h.store.getSnapshot().current);
    vi.spyOn(h.api, 'write').mockRejectedValueOnce(
      new ConfigurationApiError(422, 'Saved resource missing'),
    );
    await expect(
      h.store.restoreProfileField(
        h.store.getSnapshot().activeProfile!,
        'analysis.plies',
      ),
    ).rejects.toThrow('Saved resource missing');
    expect(h.store.getSnapshot().current).toEqual(before);
    expect(h.store.getSnapshot().busy).toBe(false);
  });
  it('rejects a reset from an outdated active-profile token without writing', async () => {
    const h = harness();
    await h.store.load();
    const old = h.store.getSnapshot().activeProfile!;
    h.commitOtherProfile();
    await h.store.refresh();
    await expect(
      h.store.restoreProfileField(old, 'analysis.plies'),
    ).rejects.toThrow();
    expect(h.calls).toEqual([]);
    expect(h.store.getSnapshot().activeProfile?.id).toBe('p2');
  });
  it('retains concurrent local edits accepted while a field reset is saving', async () => {
    const h = harness();
    await h.store.load();
    await h.store.applyCurrent((c) => ({
      ...c,
      analysis: { ...c.analysis, plies: 4 },
    }));
    h.calls.length = 0;
    h.hold();
    const resetting = h.store.restoreProfileField(
      h.store.getSnapshot().activeProfile!,
      'analysis.plies',
    );
    await vi.waitFor(() => expect(h.calls).toEqual(['write']));
    h.store.updateCurrent((c) => ({
      ...c,
      general: { ...c.general, player_name: 'Concurrent edit' },
    }));
    h.release();
    await resetting;
    expect(h.store.getSnapshot().current?.analysis.plies).toBe(2);
    expect(h.store.getSnapshot().current?.general.player_name).toBe(
      'Concurrent edit',
    );
    await h.store.flush();
    expect(
      h.store.getSnapshot().activeProfile?.configuration.general.player_name,
    ).toBe('');
  });
  it('rejects a reset when the saved revision of the same profile changed', async () => {
    const h = harness();
    await h.store.load();
    const old = h.store.getSnapshot().activeProfile!;
    const remote = await h.api.read();
    remote.revision++;
    remote.active_profile.revision++;
    vi.spyOn(h.api, 'read').mockResolvedValueOnce(remote);
    await h.store.refresh();
    await expect(
      h.store.restoreProfileField(old, 'analysis.plies'),
    ).rejects.toThrow();
    expect(h.calls).toEqual([]);
  });
  it('does not reset a field when flushing other pending changes conflicts', async () => {
    const h = harness();
    await h.store.load();
    h.store.updateCurrent((c) => ({
      ...c,
      analysis: { ...c.analysis, plies: 4 },
      general: { ...c.general, player_name: 'Pending' },
    }));
    h.conflict();
    await expect(
      h.store.restoreProfileField(
        h.store.getSnapshot().activeProfile!,
        'analysis.plies',
      ),
    ).rejects.toThrow('Conflict');
    expect(h.store.getSnapshot().current?.analysis.plies).toBe(4);
    expect(h.store.getSnapshot().current?.general.player_name).toBe('Pending');
    expect(h.store.getSnapshot().status).toBe('conflict');
    expect(h.calls).toEqual(['write']);
    expect(h.storage.size).toBe(1);
  });
  for (const changedServer of [false, true])
    it(`journals busy edits across pagehide when server revision ${changedServer ? 'changes' : 'is unchanged'}`, async () => {
      const h = harness();
      await h.store.load();
      h.holdMutation();
      const switching = h.store
        .activateProfile(
          { ...h.store.getSnapshot().activeProfile!, id: 'p2' },
          'keep',
        )
        .catch(() => {});
      await vi.waitFor(() =>
        expect(h.calls).toEqual(['/profiles/p2/activate']),
      );
      h.store.updateCurrent((c) => ({
        ...c,
        general: { ...c.general, player_name: 'Edited during activation' },
      }));
      h.store.updateCurrent((c) => ({
        ...c,
        appearance: { ...c.appearance, mode: 'dark' },
      }));
      expect(h.store.getSnapshot().current?.general.player_name).toBe('');
      const beforeHide = JSON.parse(
        h.storage.get('bgtutor.configuration-pending.v1')!,
      );
      expect(beforeHide).toMatchObject({
        base_revision: 1,
        configuration: {
          general: { player_name: 'Edited during activation' },
          appearance: { mode: 'dark' },
        },
      });
      h.store.dispose();
      if (changedServer) h.commitOtherProfile();
      const restarted = h.newStore();
      await restarted.load();
      expect(restarted.getSnapshot().current?.general.player_name).toBe(
        'Edited during activation',
      );
      expect(restarted.getSnapshot().current?.appearance.mode).toBe('dark');
      expect(restarted.getSnapshot().status).toBe(
        changedServer ? 'conflict' : 'ready',
      );
      expect(h.calls).toEqual(
        changedServer
          ? ['/profiles/p2/activate']
          : ['/profiles/p2/activate', 'write'],
      );
      h.releaseMutation();
      await switching;
      restarted.dispose();
    });
  it('clears a transient same-revision read error after a successful refresh', async () => {
    const h = harness();
    h.failListOnce();
    await h.store.load();
    expect(h.store.getSnapshot().status).toBe('error');
    await h.store.refresh();
    expect(h.store.getSnapshot().status).toBe('ready');
    expect(h.store.getSnapshot().error).toBeNull();
  });
  for (const action of ['reload', 'saveNew'] as const)
    it(`refreshes engine availability after ${action} conflict recovery`, async () => {
      const h = harness();
      h.setAvailable(false);
      await h.store.load();
      expect(h.store.getSnapshot().availabilityIssues).toMatchObject([
        { path: 'analysis.engine', code: 'unavailable_engine' },
      ]);
      h.setAvailable(true);
      if (action === 'reload') await h.store.reloadServer();
      else await h.store.saveConflictAsNewProfile('Recovered');
      expect(h.store.getSnapshot().availabilityIssues).toEqual([]);
    });
  it('keeps changed current configuration without overwriting the profile snapshot', async () => {
    const h = harness();
    await h.store.load();
    h.store.updateCurrent((c) => ({
      ...c,
      general: { ...c.general, player_name: 'Changed' },
    }));
    await h.store.flush();
    expect(h.store.getSnapshot().current?.general.player_name).toBe('Changed');
    expect(
      h.store.getSnapshot().activeProfile?.configuration.general.player_name,
    ).toBe('');
    expect(h.store.getSnapshot().dirty).toBe(true);
    expect(h.storage.size).toBe(0);
  });
  it('serializes autosave and activation', async () => {
    const h = harness();
    await h.store.load();
    h.hold();
    h.store.updateCurrent((c) => ({
      ...c,
      general: { ...c.general, player_name: 'Changed' },
    }));
    const saving = h.store.flush();
    await vi.waitFor(() => expect(h.calls).toEqual(['write']));
    const activating = h.store.activateProfile(
      { ...h.store.getSnapshot().activeProfile!, id: 'p2' },
      'keep',
    );
    expect(h.calls).toEqual(['write']);
    h.release();
    await saving;
    await activating;
    expect(h.calls).toEqual(['write', '/profiles/p2/activate']);
    expect(h.store.getSnapshot().activeProfile?.id).toBe('p2');
  });
  it('retains local work and recovery journal on revision conflict', async () => {
    const h = harness();
    await h.store.load();
    h.store.updateCurrent((c) => ({
      ...c,
      general: { ...c.general, player_name: 'Local' },
    }));
    h.conflict();
    await expect(h.store.flush()).rejects.toThrow('Conflict');
    expect(h.store.getSnapshot().status).toBe('conflict');
    expect(h.store.getSnapshot().current?.general.player_name).toBe('Local');
    expect(h.storage.size).toBe(1);
  });
  it('replays a journal only when its server base revision matches', async () => {
    const h = harness();
    const pending = structuredClone(defaultConfiguration);
    pending.general.player_name = 'Recovered';
    h.storage.set(
      'bgtutor.configuration-pending.v1',
      JSON.stringify({ base_revision: 1, configuration: pending }),
    );
    await h.store.load();
    expect(h.store.getSnapshot().current?.general.player_name).toBe(
      'Recovered',
    );
    expect(h.calls).toEqual(['write']);
    expect(h.storage.size).toBe(0);
  });
  it('does not apply an unaccepted settings dialog draft', async () => {
    const h = harness();
    await h.store.load();
    h.conflict();
    await expect(
      h.store.applyCurrent((c) => ({
        ...c,
        general: { ...c.general, player_name: 'Rejected' },
      })),
    ).rejects.toThrow();
    expect(h.store.getSnapshot().current?.general.player_name).toBe('');
    expect(h.store.getSnapshot().dirty).toBe(false);
  });
  it('keeps a mismatched journal base in conflict across another restart', async () => {
    const h = harness();
    h.conflict();
    const pending = structuredClone(defaultConfiguration);
    pending.general.player_name = 'Local';
    h.storage.set(
      'bgtutor.configuration-pending.v1',
      JSON.stringify({ base_revision: 1, configuration: pending }),
    );
    await h.store.load();
    expect(h.store.getSnapshot().status).toBe('conflict');
    expect(JSON.parse([...h.storage.values()][0]).base_revision).toBe(1);
    const reloaded = h.newStore();
    await reloaded.load();
    expect(reloaded.getSnapshot().status).toBe('conflict');
    expect(h.calls).toEqual([]);
  });
  it('preserves an unread recovery journal during provider startup cleanup', async () => {
    const h = harness();
    const pending = structuredClone(defaultConfiguration);
    pending.general.player_name = 'Recovered';
    h.storage.set(
      'bgtutor.configuration-pending.v1',
      JSON.stringify({ base_revision: 1, configuration: pending }),
    );
    const loading = h.store.load();
    h.store.dispose();
    await loading;
    expect(h.store.getSnapshot().current?.general.player_name).toBe(
      'Recovered',
    );
  });
  it('replays visible user changes after a held profile operation', async () => {
    const h = harness();
    await h.store.load();
    h.hold();
    h.store.updateCurrent((c) => ({
      ...c,
      general: { ...c.general, player_name: 'First' },
    }));
    const switching = h.store.activateProfile(
      { ...h.store.getSnapshot().activeProfile!, id: 'p2' },
      'keep',
    );
    await vi.waitFor(() => expect(h.calls).toEqual(['write']));
    h.store.updateCurrent((c) => ({
      ...c,
      general: { ...c.general, player_name: 'During switch' },
    }));
    h.release();
    await switching;
    expect(h.store.getSnapshot().current?.general.player_name).toBe(
      'During switch',
    );
    await h.store.flush();
  });
});
