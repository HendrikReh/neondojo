import { afterEach, describe, expect, it, vi } from 'vitest';
import { defaultConfiguration } from './configuration-model';
import {
  applySettingsDraft,
  settingsFromConfiguration,
  saveInstallationSettings,
} from './configuration-settings';
import type { AppSettings } from './settings-api';

const installed: AppSettings = {
  version: 4,
  player_name: 'Before',
  analysis_engine: 'gnubg',
  analysis_plies: 3,
  play_engine: 'gnubg',
  play_plies: 1,
  gnubg_path: '/local/gnubg',
  bearoff_direction: 'right',
  default_play_mode: 'coach',
  tutor_threshold: 0.04,
  training_auto_analysis: true,
};

describe('settings/profile boundary', () => {
  afterEach(() => vi.unstubAllGlobals());
  it('uses fresh projection metadata for a machine-path-only write', async () => {
    const latest = { ...installed, version: 8, analysis_plies: 4 };
    const bodies: unknown[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (_url, options) => {
        if (!options?.method) return Response.json(latest);
        const body = JSON.parse(options.body);
        bodies.push(body);
        return Response.json({ ...body, version: 9 });
      }),
    );
    await saveInstallationSettings(installed, {
      ...installed,
      gnubg_path: '/new/gnubg',
    });
    expect(bodies).toEqual([{ ...latest, gnubg_path: '/new/gnubg' }]);
  });

  it('retains a concurrent machine-path change instead of overwriting it', async () => {
    const fetcher = vi.fn(async () =>
      Response.json({ ...installed, version: 8, gnubg_path: '/other/gnubg' }),
    );
    vi.stubGlobal('fetch', fetcher);
    await expect(
      saveInstallationSettings(installed, {
        ...installed,
        gnubg_path: '/new/gnubg',
      }),
    ).rejects.toMatchObject({ status: 409 });
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it('applies a thread limit and preserves concurrent edits to the other engine', async () => {
    const latest = {
      ...installed,
      version: 8,
      open_sage_threads: 8,
      open_sage_play_threads: 3,
    };
    vi.stubGlobal(
      'fetch',
      vi.fn(async (_url, options) => {
        if (!options?.method) return Response.json(latest);
        return Response.json(JSON.parse(options.body));
      }),
    );
    const saved = await saveInstallationSettings(installed, {
      ...installed,
      gnubg_threads: 3,
      gnubg_play_threads: 2,
      parallel_analysis_jobs: 4,
    });
    expect(saved).toEqual({
      ...latest,
      gnubg_threads: 3,
      gnubg_play_threads: 2,
      parallel_analysis_jobs: 4,
    });
  });
  it('projects the current configuration without changing the machine installation', () => {
    const config = structuredClone(defaultConfiguration);
    config.general.player_name = 'Training';
    config.analysis.plies = 4;
    config.play.plies = 2;
    config.appearance.bearoff_direction = 'left';
    expect(settingsFromConfiguration(config, installed)).toMatchObject({
      version: 4,
      gnubg_path: '/local/gnubg',
      player_name: 'Training',
      analysis_plies: 4,
      play_plies: 2,
      bearoff_direction: 'left',
    });
  });

  it('applying a dialog draft preserves filters and excludes machine metadata', () => {
    const current = structuredClone(defaultConfiguration);
    current.library.filters.q = 'Alice';
    const updated = applySettingsDraft(current, {
      settings: {
        ...installed,
        player_name: 'After',
        gnubg_path: '/different/path',
      },
      locale: 'de',
      theme: 'walnut',
      appearance: { theme: 'paper', mode: 'dark' },
      sounds: false,
    });
    expect(updated.general).toMatchObject({
      player_name: 'After',
      locale: 'de',
      play_sounds: false,
    });
    expect(updated.appearance).toMatchObject({
      board_theme: 'walnut',
      ui_theme: 'paper',
      mode: 'dark',
    });
    expect(updated.library.filters.q).toBe('Alice');
    expect(JSON.stringify(updated)).not.toContain('/different/path');
    expect(current.general.player_name).not.toBe('After');
  });
});
