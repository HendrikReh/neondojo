import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, expect, it, vi } from 'vitest';
import MatchDatabase from './MatchDatabase';
import { I18nProvider } from './shared/i18n/i18n';
import {
  LIBRARY_DATA_COLUMNS,
  VISIBLE_COLUMNS_KEY,
} from './library-column-preferences';

afterEach(() => vi.unstubAllGlobals());

it('exposes independent sort controls for every match data field, including numeric ID', () => {
  vi.stubGlobal('localStorage', {
    getItem: (key: string) =>
      key === VISIBLE_COLUMNS_KEY
        ? JSON.stringify({ version: 1, columns: LIBRARY_DATA_COLUMNS })
        : 'en',
  });
  const noop = () => {};
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchDatabase, {
        revision: 0,
        annotationRevision: 0,
        databaseId: '',
        visible: true,
        onDatabase: noop,
        onOpen: noop,
        onPosition: noop,
        onAnnotation: noop,
        onImport: noop,
        onImportFolder: noop,
        onAnalyzeMatches: noop,
        uploading: false,
        onChanged: noop,
      }),
    }),
  );
  for (const key of [
    'id',
    'white',
    'white_pr',
    'black',
    'black_pr',
    'match_length',
    'score',
    'played_at',
    'created_at',
    'tournament',
    'position_count',
    'analysis_status',
    'tags',
    'comment',
  ])
    expect(html).toContain(`data-sort="${key}"`);
  expect(html).not.toContain('data-sort="format"');
  expect(html).toContain('aria-label="Sort by ID"');
  expect(html).toContain('aria-sort="descending"');
  expect(html).toContain('aria-label="Sort by White · PR"');
  expect(html).toContain('aria-label="Sort by Black · PR"');
});
