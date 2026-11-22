import { describe, expect, it } from 'vitest';
import html from '../../../index.html?raw';
import {
  defaultUiAppearance,
  readUiAppearance,
  saveUiAppearance,
  resolveUiMode,
  observeSystemAppearance,
  applyUiAppearance,
  uiAppearanceStorageKey,
} from './ui-appearance';

function memoryStorage(raw: string | null = null) {
  const values = new Map<string, string>();
  if (raw !== null) values.set('neondojo.ui-appearance', raw);
  values.set('neondojo.theme', 'walnut');
  return {
    values,
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  };
}

describe('interface appearance preferences', () => {
  it('defaults to Graphite following the system without replacing the board theme', () => {
    const storage = memoryStorage();
    expect(readUiAppearance(storage)).toEqual({
      theme: 'graphite',
      mode: 'system',
    });
    expect(storage.values.get('neondojo.theme')).toBe('walnut');
  });

  it('persists every supported combination in a separate versioned namespace', () => {
    const storage = memoryStorage();
    for (const theme of ['graphite', 'club', 'paper'] as const) {
      for (const mode of ['light', 'dark', 'system'] as const) {
        saveUiAppearance({ theme, mode }, storage);
        expect(readUiAppearance(storage)).toEqual({ theme, mode });
        expect(JSON.parse(storage.values.get(uiAppearanceStorageKey)!)).toEqual(
          { version: 1, theme, mode },
        );
      }
    }
    expect(storage.values.get('neondojo.theme')).toBe('walnut');
  });

  it('rejects corrupt, unsupported and incomplete stored preferences', () => {
    for (const raw of [
      'bad json',
      'null',
      '[]',
      '{}',
      '{"version":2,"theme":"club","mode":"dark"}',
      '{"version":1,"theme":"other","mode":"light"}',
      '{"version":1,"theme":"club","mode":"sepia"}',
      '{"version":1,"theme":"paper"}',
    ]) {
      expect(readUiAppearance(memoryStorage(raw))).toEqual(defaultUiAppearance);
    }
  });

  it('keeps appearance usable when persistent storage is unavailable', () => {
    expect(
      readUiAppearance({
        getItem: () => {
          throw new Error('blocked');
        },
      }),
    ).toEqual(defaultUiAppearance);
    expect(() =>
      saveUiAppearance(
        { theme: 'paper', mode: 'dark' },
        {
          setItem: () => {
            throw new Error('blocked');
          },
        },
      ),
    ).not.toThrow();
  });

  it('uses system darkness only for the System preference', () => {
    expect(resolveUiMode('system', true)).toBe('dark');
    expect(resolveUiMode('system', false)).toBe('light');
    expect(resolveUiMode('light', true)).toBe('light');
    expect(resolveUiMode('dark', false)).toBe('dark');
  });

  it('updates System appearance and releases its listener on cleanup', () => {
    let listener: ((event: { matches: boolean }) => void) | undefined;
    const values: boolean[] = [];
    const query = {
      matches: false,
      addEventListener: (_event: string, callback: typeof listener) => {
        listener = callback;
      },
      removeEventListener: (_event: string, callback: typeof listener) => {
        if (listener === callback) listener = undefined;
      },
    };
    const stop = observeSystemAppearance(values.push.bind(values), query);
    expect(values).toEqual([false]);
    listener?.({ matches: true });
    expect(values).toEqual([false, true]);
    stop();
    expect(listener).toBeUndefined();
  });

  it('applies resolved native control scheme without changing the board theme', () => {
    const root = {
      dataset: { boardTheme: 'walnut' } as Record<string, string>,
      style: { colorScheme: '' },
    };
    applyUiAppearance({ theme: 'paper', mode: 'system' }, true, root);
    expect(root.dataset).toMatchObject({
      uiTheme: 'paper',
      uiMode: 'dark',
      uiPreference: 'system',
      boardTheme: 'walnut',
    });
    expect(root.style.colorScheme).toBe('dark');
  });
});

describe('first paint bootstrap', () => {
  const script =
    html.match(
      /<script id="ui-appearance-bootstrap">([\s\S]*?)<\/script>/,
    )?.[1] ?? '';
  it('applies persisted darkness before the app or styles load', () => {
    expect(script).not.toBe('');
    const root = {
      dataset: {} as Record<string, string>,
      style: { colorScheme: '' },
    };
    const storage = memoryStorage(
      JSON.stringify({ version: 1, theme: 'club', mode: 'system' }),
    );
    new Function('document', 'localStorage', 'window', script)(
      { documentElement: root },
      storage,
      { matchMedia: () => ({ matches: true }) },
    );
    expect(root.dataset).toMatchObject({
      uiTheme: 'club',
      uiMode: 'dark',
      uiPreference: 'system',
    });
    expect(root.style.colorScheme).toBe('dark');
    expect(html.indexOf('ui-appearance-bootstrap')).toBeLessThan(
      html.indexOf('/src/styles/ui-appearance.css'),
    );
  });
  it('uses safe defaults when storage access fails', () => {
    const root = {
      dataset: {} as Record<string, string>,
      style: { colorScheme: '' },
    };
    new Function('document', 'localStorage', 'window', script)(
      { documentElement: root },
      {
        getItem: () => {
          throw new Error('blocked');
        },
      },
      { matchMedia: () => ({ matches: true }) },
    );
    expect(root.dataset).toMatchObject({ uiTheme: 'graphite', uiMode: 'dark' });
  });
});
