import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  checkGnubg,
  loadSageInstallation,
  saveSageInstallation,
  loadEngines,
  loadSettings,
  saveSettings,
  SettingsApiError,
  type AppSettings,
} from './settings-api';

const settings: AppSettings = {
  version: 7,
  player_name: 'Zoë',
  analysis_engine: 'gnubg',
  analysis_plies: 0,
  play_engine: 'gnubg',
  play_plies: 2,
  gnubg_path: '/usr/local/bin/gnubg',
};

afterEach(() => vi.unstubAllGlobals());

describe('settings API', () => {
  it('validates saved feedback modes and the independent tutor threshold', async () => {
    for (const invalid of [
      { default_play_mode: 'expert' },
      { tutor_threshold: 0 },
      { tutor_threshold: -0.01 },
      { tutor_threshold: 2 },
      { tutor_threshold: '0.04' },
      { training_auto_analysis: 'yes' },
    ]) {
      vi.stubGlobal('fetch', async () =>
        Response.json({ ...settings, ...invalid }),
      );
      await expect(loadSettings()).rejects.toBeInstanceOf(SettingsApiError);
    }
    for (const mode of ['competition', 'training', 'coach', 'tutor']) {
      vi.stubGlobal('fetch', async () =>
        Response.json({
          ...settings,
          default_play_mode: mode,
          tutor_threshold: 0.02,
          training_auto_analysis: false,
        }),
      );
      expect(await loadSettings()).toMatchObject({
        default_play_mode: mode,
        tutor_threshold: 0.02,
        training_auto_analysis: false,
      });
    }
  });
  it('accepts both bear-off directions and rejects unsupported values', async () => {
    for (const direction of ['left', 'right']) {
      vi.stubGlobal('fetch', async () =>
        Response.json({ ...settings, bearoff_direction: direction }),
      );
      expect((await loadSettings()).bearoff_direction).toBe(direction);
    }
    vi.stubGlobal('fetch', async () =>
      Response.json({ ...settings, bearoff_direction: 'sideways' }),
    );
    await expect(loadSettings()).rejects.toBeInstanceOf(SettingsApiError);
  });
  it.each([3, 4])(
    'accepts a %i-ply analysis profile while retaining the separate play profile',
    async (plies) => {
      vi.stubGlobal('fetch', async () =>
        Response.json({ ...settings, analysis_plies: plies }),
      );
      expect((await loadSettings()).analysis_plies).toBe(plies);
    },
  );
  it('rejects analysis depth five and unsupported play depth three', async () => {
    for (const invalid of [{ analysis_plies: 5 }, { play_plies: 3 }]) {
      vi.stubGlobal('fetch', async () =>
        Response.json({ ...settings, ...invalid }),
      );
      await expect(loadSettings()).rejects.toBeInstanceOf(SettingsApiError);
    }
  });
  it('loads real saved settings and forwards cancellation', async () => {
    const abort = new AbortController();
    const fetch = vi.fn(async () => Response.json(settings));
    vi.stubGlobal('fetch', fetch);
    expect(await loadSettings(abort.signal)).toEqual(settings);
    expect(fetch).toHaveBeenCalledWith('/api/settings', {
      signal: abort.signal,
    });
  });

  it('sends separate analysis/play profiles with the saved version', async () => {
    const fetch = vi.fn(async () => Response.json({ ...settings, version: 8 }));
    vi.stubGlobal('fetch', fetch);
    expect((await saveSettings(settings)).version).toBe(8);
    const [url, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/settings');
    expect(init.method).toBe('PUT');
    expect(JSON.parse(String(init.body))).toEqual(settings);
    expect(new Headers(init.headers).get('content-type')).toBe(
      'application/json',
    );
  });

  it('retains conflict status and backend diagnostic for a save error', async () => {
    vi.stubGlobal('fetch', async () =>
      Response.json({ error: 'Version changed' }, { status: 409 }),
    );
    await expect(saveSettings(settings)).rejects.toMatchObject({
      status: 409,
      message: 'Version changed',
    });
  });

  it('does not invent empty settings after malformed successful responses', async () => {
    vi.stubGlobal('fetch', async () => Response.json({ version: 1 }));
    await expect(loadSettings()).rejects.toBeInstanceOf(SettingsApiError);
  });

  it('reads engine capability profiles including an unavailable future engine', async () => {
    const engines = [
      {
        id: 'gnubg',
        name: 'GNU Backgammon',
        available: true,
        version: '1.08',
        plies: [0, 1, 2],
      },
      {
        id: 'bgsage',
        name: 'BGSage',
        available: false,
        version: null,
        plies: [],
      },
    ];
    vi.stubGlobal('fetch', async () => Response.json(engines));
    expect(await loadEngines()).toEqual(engines);
  });

  it('reads saved thread counts without imposing a client-side maximum', async () => {
    const saved = {
      ...settings,
      gnubg_threads: 64,
      gnubg_play_threads: 32,
      open_sage_threads: 64,
      open_sage_play_threads: 32,
    };
    vi.stubGlobal('fetch', async () => Response.json(saved));
    expect(await loadSettings()).toEqual(saved);
  });

  it('rejects playing thread counts below the engine minimum', async () => {
    for (const invalid of [
      { gnubg_play_threads: 0 },
      { open_sage_play_threads: 1 },
      { open_sage_play_threads: 2.5 },
    ]) {
      vi.stubGlobal('fetch', async () =>
        Response.json({ ...settings, ...invalid }),
      );
      await expect(loadSettings()).rejects.toMatchObject({ status: 200 });
    }
  });

  it('rejects an invalid detected parallel thread maximum', async () => {
    vi.stubGlobal('fetch', async () =>
      Response.json([
        {
          id: 'gnubg',
          name: 'GNUbg',
          available: true,
          version: '1.08',
          plies: [2],
          max_parallel_threads: 0,
        },
      ]),
    );
    await expect(loadEngines()).rejects.toBeInstanceOf(SettingsApiError);
  });

  it('checks the backend executable path and surfaces its actual version', async () => {
    const fetch = vi.fn(async () =>
      Response.json({ version: 'GNU Backgammon 1.08.003' }),
    );
    vi.stubGlobal('fetch', fetch);
    expect(await checkGnubg('/opt/bin/gnubg')).toEqual({
      version: 'GNU Backgammon 1.08.003',
    });
    const [url, init] = fetch.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('/api/engines/gnubg/check');
    expect(JSON.parse(String(init.body))).toEqual({ path: '/opt/bin/gnubg' });
  });

  it('keeps AbortError distinct from a network failure', async () => {
    vi.stubGlobal('fetch', async () => {
      throw new DOMException('Aborted', 'AbortError');
    });
    await expect(loadSettings()).rejects.toMatchObject({ name: 'AbortError' });
  });
});

it('rejects malformed local installation metadata without crashing the settings form', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(Response.json([])));
  await expect(loadSageInstallation()).rejects.toBeInstanceOf(SettingsApiError);
});
it('keeps local installation revision and path separate from portable preferences', async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValue(
      Response.json({
        version: 2,
        python_path: '/local/python',
        snapshot: null,
      }),
    );
  vi.stubGlobal('fetch', fetcher);
  expect((await saveSageInstallation(1, '/local/python')).version).toBe(2);
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({
    version: 1,
    path: '/local/python',
  });
});
