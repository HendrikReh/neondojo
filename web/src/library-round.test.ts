import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, describe, expect, it, vi } from 'vitest';
import LibraryFilters from './LibraryFilters';
import { emptyFilters, libraryParams } from './library-api';
import { I18nProvider, type Locale } from './i18n';

afterEach(() => vi.unstubAllGlobals());

describe('library round filter', () => {
  it('starts empty and forwards trimmed literal round text with existing filters', () => {
    expect(emptyFilters()).toHaveProperty('round', '');
    const filters = {
      ...emptyFilters(),
      round: '  Final 100%_A\\B  ',
      tournament: 'Autumn Open',
    };
    const params = libraryParams('database', filters, 'name', 'asc', 2);
    expect(params.get('round')).toBe('Final 100%_A\\B');
    expect(params.get('tournament')).toBe('Autumn Open');
    expect(params.get('database_id')).toBe('database');
    expect(params.get('page')).toBe('2');
    expect(libraryParams('', emptyFilters(), 'name', 'asc').has('round')).toBe(
      false,
    );
  });

  it.each<[Locale, string]>([
    ['en', 'Round'],
    ['de', 'Runde'],
    ['es', 'Ronda'],
    ['fr', 'Ronde'],
    ['pl', 'Runda'],
    ['it', 'Turno'],
  ])('renders the round input in %s', (locale, label) => {
    vi.stubGlobal('localStorage', {
      getItem: (key: string) => (key === 'bgtutor.locale' ? locale : null),
    });
    const markup = renderToStaticMarkup(
      createElement(I18nProvider, {
        children: createElement(LibraryFilters, {
          filters: { ...emptyFilters(), round: 'Final' },
          onChange: () => {},
          onClear: () => {},
        }),
      }),
    );
    expect(markup).toContain(
      `<label><span>${label}</span><input type="text" value="Final"/></label>`,
    );
  });
});
