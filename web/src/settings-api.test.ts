import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  checkGnubg,
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
