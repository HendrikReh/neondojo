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

const checkerBook: BookLookup = {
  provider: 'backgammondb',
  hit: true,
  provenance: {
    provider: 'backgammondb',
    engine: 'GNUbg',
    evaluation_kind: 'evaluation',
    equity_kind: 'normalized_cubeful',
    context: 'money',
  },
  checker: {
    plies: 3,
    candidates: [
      { rank: 1, notation: '8/5 6/5', equity: 0.2, probabilities: [0.55] },
      { rank: 2, notation: '13/10 6/5', equity: 0.1, probabilities: [0.52] },
    ],
  },
};
test('book comparison preserves the authoritative normalized equity unit', () => {
  const html = render(checkerBook);
  expect(html).toContain('Normalized cubeful');
  expect(html).toContain('>0.100</td>');
  expect(html).not.toContain('>Cubeful</span>');
});
test('book comparison without provenance leaves its unit and difference unavailable', () => {
  const html = render({ ...checkerBook, provenance: null });
  expect(html).toContain('Unit unknown');
  expect(html).not.toContain('>0.100</td>');
});
const emptyRecord: import('./source-analysis-api').SourceAnalysis = {
  id: 'record',
  match_uuid: 'match',
  position_uuid: null,
  source_format: 'sgf',
  kind: 'checker',
  engine: 'GNUbg',
  engine_version: '1.08.003',
  depth: '2-ply',
  depth_plies: 2,
  checker_ply: 1,
  dice: [3, 1],
  game_no: 1,
  gnubg_match_id: null,
  position_id: null,
  source_move_no: 1,
  player: 0,
  played: '8/5 6/5',
  method: 'evaluation',
  candidates: [],
  cube: null,
  raw: {},
};
function renderRecords(
  records: (typeof emptyRecord)[],
  preferredKind?: 'checker' | 'cube',
) {
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(SourceAnalysisView, {
        source: 'file',
        records,
        preferredKind,
      }),
    }),
  );
}
test('an empty checker record keeps provenance and explicitly reports unavailable candidates', () => {
  const html = renderRecords([emptyRecord]);
  expect(html).toContain('GNUbg 1.08.003');
  expect(html).toContain('2-ply');
  expect(html).toContain('evaluation');
  expect(html).toContain('No candidate values recorded.');
  expect(html).not.toContain('<table');
});
test('a cube review defaults to the cube record when checker and cube records share a move', () => {
  const html = renderRecords(
    [
      emptyRecord,
      {
        ...emptyRecord,
        id: 'cube',
        kind: 'cube',
        cube: {
          no_double: 0.15,
          double_take: 0.1,
          double_pass: 1,
          equity_kind: 'cubeful',
        },
      },
    ],
    'cube',
  );
  expect(html).toContain('Cube decision before this roll');
  expect(html).toContain('+0.150');
  expect(html).not.toContain('No candidate values recorded.');
});
