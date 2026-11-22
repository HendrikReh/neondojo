import { describe, expect, it } from 'vitest';
import { catalogs } from './i18n-catalogs';
import { projectMove } from './move-preview';
import {
  AppError,
  formatLocalizedError,
  formatMessage,
  interpolate,
  localizeDecision,
  numberFormat,
  languages,
  type Locale,
  placeholders,
  pluralCategory,
  readLocale,
  saveLocale,
  selectPluralForms,
  translate,
  validateCatalogs,
  validatePluralForms,
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
    expect(values.get('neondojo.locale')).toBe('pl');
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
      '123 pipsy · 2 zdjęte',
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

describe('plural forms for counted nouns', () => {
  const twoForms = '{count, plural, one {# card} other {# cards}}';
  const cards: Record<Locale, string> = {
    en: twoForms,
    de: '{count, plural, one {# Karte} other {# Karten}}',
    es: '{count, plural, one {# tarjeta} other {# tarjetas}}',
    fr: '{count, plural, one {# carte} other {# cartes}}',
    it: '{count, plural, one {# scheda} other {# schede}}',
    pl: '{count, plural, one {# karta} few {# karty} many {# kart}}',
  };
  const expected: Record<Locale, string[]> = {
    en: ['0 cards', '1 card', '2 cards', '5 cards', '22 cards'],
    de: ['0 Karten', '1 Karte', '2 Karten', '5 Karten', '22 Karten'],
    es: ['0 tarjetas', '1 tarjeta', '2 tarjetas', '5 tarjetas', '22 tarjetas'],
    // French counts zero as singular.
    fr: ['0 carte', '1 carte', '2 cartes', '5 cartes', '22 cartes'],
    it: ['0 schede', '1 scheda', '2 schede', '5 schede', '22 schede'],
    pl: ['0 kart', '1 karta', '2 karty', '5 kart', '22 karty'],
  };
  const counts = [0, 1, 2, 5, 22];

  it.each(Object.keys(cards) as Locale[])(
    'chooses the form for 0, 1, 2, 5 and 22 in %s',
    (locale) => {
      expect(
        counts.map((count) => interpolate(cards[locale], { count }, locale)),
      ).toEqual(expected[locale]);
    },
  );

  it('uses the plural categories of each language', () => {
    expect(counts.map((count) => pluralCategory('pl', count))).toEqual([
      'many',
      'one',
      'few',
      'many',
      'few',
    ]);
    expect(counts.map((count) => pluralCategory('de', count))).toEqual([
      'other',
      'one',
      'other',
      'other',
      'other',
    ]);
    expect(pluralCategory('fr', 0)).toBe('one');
  });

  it('writes the count in the language and keeps other parameters', () => {
    const moved =
      '{count, plural, one {# Karte} other {# Karten}} nach „{name}“ verschoben.';
    expect(interpolate(moved, { count: 1, name: 'Bearoff' }, 'de')).toBe(
      '1 Karte nach „Bearoff“ verschoben.',
    );
    expect(interpolate(moved, { count: 12345, name: '{count}' }, 'de')).toBe(
      '12.345 Karten nach „{count}“ verschoben.',
    );
    expect(
      formatMessage(
        'en',
        '{matches, plural, one {# match} other {# matches}} of {total}',
        { matches: 1, total: 2000 },
      ),
    ).toBe('1 match of 2,000');
    expect(
      selectPluralForms(
        'pl',
        'Usuń {count, plural, one {# fiszkę} few {# fiszki} many {# fiszek}} z „{name}”',
        { count: 3 },
      ),
    ).toBe('Usuń 3 fiszki z „{name}”');
    expect(placeholders(moved)).toEqual(['count', 'name']);
  });

  it('falls back to other, then many, for categories without their own form', () => {
    expect(interpolate(twoForms, { count: 1_000_000 }, 'es')).toBe(
      '1.000.000 cards',
    );
    expect(interpolate(cards.pl, { count: 1.5 }, 'pl')).toBe('1,5 kart');
  });

  it('rejects incomplete or misused plural messages', () => {
    expect(() => interpolate(twoForms, { count: 1 })).toThrow('locale');
    expect(() => interpolate(twoForms, { count: '1' }, 'en')).toThrow(
      'must be a number',
    );
    expect(() => validatePluralForms('pl', twoForms)).toThrow('few');
    expect(() =>
      validatePluralForms('de', '{count, plural, other {# Karten}}'),
    ).toThrow('one');
    expect(() =>
      selectPluralForms('en', '{count, plural, some {x} other {y}}', {
        count: 1,
      }),
    ).toThrow('Unknown plural category');
    expect(() => validateCatalogs(catalogs)).not.toThrow();
    const broken = {
      ...catalogs,
      pl: { ...catalogs.pl, 'board.points': twoForms },
    };
    expect(() => validateCatalogs(broken)).toThrow('few');
  });

  it('gives every plural message in the message tables a complete set of forms', () => {
    const sources = import.meta.glob<string>(
      ['./*-messages.ts', './i18n-catalogs.ts'],
      { query: '?raw', import: 'default', eager: true },
    );
    let checked = 0;
    for (const [file, source] of Object.entries(sources)) {
      for (const [literal] of source.matchAll(/'[^'\n]*, plural,[^'\n]*'/g)) {
        const template = literal.slice(1, -1);
        // The tables do not all expose their locale, so accept either the
        // one/few/many set of Polish or the one/other set of the others.
        const complete = (['pl', 'en'] as const).some((locale) => {
          try {
            validatePluralForms(locale, template);
            return true;
          } catch {
            return false;
          }
        });
        expect(complete, `${file}: ${template}`).toBe(true);
        for (const count of counts)
          expect(
            selectPluralForms(
              'pl',
              template,
              Object.fromEntries(
                placeholders(template).map((name) => [name, count]),
              ),
            ),
          ).not.toContain(', plural,');
        checked += 1;
      }
    }
    expect(checked).toBeGreaterThan(100);
  });
});

describe('numberFormat', () => {
  it('reuses one formatter per locale and options', () => {
    const options = { minimumFractionDigits: 3, maximumFractionDigits: 3 };
    expect(numberFormat('de', options)).toBe(
      numberFormat('de', { ...options }),
    );
    expect(numberFormat('de', options)).not.toBe(numberFormat('en', options));
    expect(numberFormat('de')).not.toBe(numberFormat('de', options));
    expect(numberFormat('de', options).format(0.05)).toBe('0,050');
    expect(numberFormat('en', options).format(0.05)).toBe('0.050');
  });
});
