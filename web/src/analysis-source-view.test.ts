import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { I18nProvider } from './i18n';
import SourceAnalysisView from './SourceAnalysisView';
import type { BookLookup } from './source-analysis-api';
beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const lookup: BookLookup = {
  provider: 'automatic',
  hit: true,
  checker: null,
  cube: null,
  provenance: null,
  xg_position: {
    book_version: '3.70',
    license: 'Public domain; preserve source',
    native_perspective: 'opponent_of_player_on_roll',
    perspective: 'player_on_roll',
    position_key: 'example',
    source_url: 'https://www.extremegammon.com/GetOb.aspx',
    evaluations: [
      {
        record_index: 1,
        source_author: 'Original Author',
        engine: 'eXtreme Gammon',
        engine_version: '2.10',
        method: 'rollout',
        depth: 'Rollout',
        level: 100,
        plies: null,
        equity: -0.25,
        equity_kind: 'normalized_cubeful',
        probabilities: [0.45, 0.12, 0.01, 0.55, 0.17, 0.02],
        native_equity: 0.25,
        native_probabilities: [0.01, 0.12, 0.45, 0.55, 0.17, 0.02],
        rollout: null,
      },
    ],
  },
};
const render = (book: BookLookup) =>
  renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(SourceAnalysisView, {
        records: [],
        source: 'book',
        book,
      }),
    }),
  );
test('automatic mode displays an available XG-only reference with its original author', () => {
  const html = render(lookup);
  expect(html).toContain('Original Author');
  expect(html).toContain('before the roll');
  expect(html).toContain('XG Opening Book');
  expect(html).not.toContain('class="candidates"');
});
test('explicit BackgammonDB mode disables the XG option that was not requested', () => {
  const html = render({
    ...lookup,
    provider: 'backgammondb',
    hit: false,
    xg_position: null,
  });
  const button = html.match(
    /<button[^>]*>XG position reference<\/button>/,
  )?.[0];
  expect(button).toContain('disabled');
});
