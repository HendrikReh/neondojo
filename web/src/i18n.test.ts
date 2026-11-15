import { describe, expect, it } from 'vitest';
import { catalogs } from './i18n-catalogs';
import { projectMove } from './move-preview';
import {
  AppError,
  formatLocalizedError,
  interpolate,
  localizeDecision,
  languages,
  readLocale,
  saveLocale,
  translate,
  validateCatalogs,
} from './i18n-core';

describe('language selection and translation', () => {
  it('starts in English and preserves explicit saved preferences', () => {
    expect(readLocale({ getItem: () => null })).toBe('en');
    expect(readLocale({ getItem: () => 'de' })).toBe('de');
    expect(readLocale({ getItem: () => 'fr' })).toBe('fr');
    expect(readLocale({ getItem: () => 'pt-BR' })).toBe('en');
    expect(
      readLocale({
        getItem: () => {
          throw new Error('blocked');
        },
      }),
    ).toBe('en');
  });

  it('lists all six languages alphabetically by their visible names', () => {
    expect(languages.map(({ name }) => name)).toEqual([
      'Deutsch',
      'English',
      'Español',
      'Français',
      'Italiano',
      'Polski',
    ]);
  });

  it('persists a supported language and tolerates disabled browser storage', () => {
    const values = new Map<string, string>();
    saveLocale('pl', { setItem: (key, value) => values.set(key, value) });
    expect(readLocale({ getItem: (key) => values.get(key) ?? null })).toBe(
      'pl',
    );
    expect(values.get('bgtutor.locale')).toBe('pl');
    expect(() =>
      saveLocale('es', {
        setItem: () => {
          throw new Error('blocked');
        },
      }),
    ).not.toThrow();
  });

  it('renders every board description in the selected language without changing names', () => {
    expect(translate('en', 'board.points', { count: 7 })).toBe('7 points');
    expect(translate('pl', 'board.pipsOff', { pips: 123, off: 2 })).toBe(
      '123 pipsów · 2 zdjęte',
    );
    expect(translate('fr', 'board.cube', { value: 4, owner: 'Élodie' })).toBe(
      'Videau 4, Élodie',
    );
    expect(
      translate('it', 'board.label', {
        player: 'Zoë',
        first: 3,
        second: 2,
        white: 100,
        black: 101,
      }),
    ).toContain('Zoë');
  });

  it('fails clearly on missing interpolation values and does not interpret inserted text', () => {
    expect(() =>
      interpolate('Hello {player}, roll {dice}', { player: 'Ada' }),
    ).toThrow('dice');
    expect(interpolate('{player} / {player}', { player: '{dice}' })).toBe(
      '{dice} / {dice}',
    );
    expect(interpolate('{count} points', { count: 0 })).toBe('0 points');
  });

  it('formats translated quantities for each language while leaving technical notation intact', () => {
    expect(translate('de', 'board.points', { count: 12345 })).toBe(
      '12.345 Punkte',
    );
    expect(translate('en', 'board.points', { count: 12345 })).toBe(
      '12,345 points',
    );
    expect(localizeDecision('es', 'No double')).toBe('No doblar');
    expect(localizeDecision('fr', 'Take')).toBe('Accepter');
    expect(localizeDecision('it', 'Cannot move')).toBe('Nessuna mossa');
    expect(localizeDecision('pl', '24/18(2) 13/7*')).toBe('24/18(2) 13/7*');
  });

  it('checks key and placeholder coverage for all six complete catalogs', () => {
    expect(Object.keys(catalogs).sort()).toEqual([
      'de',
      'en',
      'es',
      'fr',
      'it',
      'pl',
    ]);
    expect(() => validateCatalogs(catalogs)).not.toThrow();
    const broken = {
      ...catalogs,
      en: { ...catalogs.en, 'board.points': '{amount} points' },
    };
    expect(() => validateCatalogs(broken)).toThrow('board.points');
  });

  it('displays the 6–3 run in the same notation while previewing the recorded path', () => {
    const position = {
      board: [
        0, -2, 0, 0, 0, 0, 5, 0, 3, 0, 0, 0, -5, 5, 0, 0, 0, -3, 0, -5, 0, 0, 0,
        0, 2, 0,
      ],
      player: 0,
      dice: [6, 3],
      notation: '24/21 21/15',
    };
    expect(localizeDecision('de', position.notation, position)).toBe('24/15');
    expect(localizeDecision('de', '24/15', position)).toBe('24/15');
    expect(projectMove(position)).toMatchObject({
      ok: true,
      steps: [
        { from: 24, to: 21, die: 3 },
        { from: 21, to: 15, die: 6 },
      ],
    });
  });

  it.each([
    ['24/21/15', '24/15'],
    ['24/21* 21/15', '24/21*/15'],
    ['24/21 21/15*', '24/15*'],
    ['bar/24 24/22', 'bar/22'],
    ['7/4 4/off', '7/off'],
    ['13/10(2) 10/7(2)', '13/7(2)'],
    ['13/10 8/5 10/7 5/2', '13/7 8/2'],
    ['24/21 24/18', '24/21 24/18'],
    ['24/21 21/15?', '24/21 21/15?'],
  ])('formats checker notation %s as %s', (original, expected) => {
    expect(localizeDecision('en', original)).toBe(expected);
  });

  it('retains a hit on an intermediate point even if the source omitted its star', () => {
    const board = Array<number>(26).fill(0);
    board[24] = 1;
    board[21] = -1;
    expect(
      localizeDecision('de', '24/21 21/15', { board, player: 0, dice: [6, 3] }),
    ).toBe('24/21*/15');
  });

  it('localizes errors at render time while preserving native diagnostic text', () => {
    const error = new AppError(
      'error.request',
      { status: 503 },
      'GNUbg unavailable',
    );
    expect(formatLocalizedError('de', error)).toContain(
      'Anfrage fehlgeschlagen (HTTP 503)',
    );
    expect(formatLocalizedError('es', error)).toContain(
      'La solicitud ha fallado (HTTP 503)',
    );
    expect(formatLocalizedError('fr', error)).toContain('GNUbg unavailable');
    expect(translate('de', 'error.fileSize')).toBe(
      'Die Datei darf höchstens 16 MiB groß sein.',
    );
  });
});
