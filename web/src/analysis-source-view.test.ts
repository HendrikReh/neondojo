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
  players?: { white: string; black: string },
) {
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(SourceAnalysisView, {
        source: 'file',
        records,
        preferredKind,
        players,
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

test('OGXM legacy empty analysis shows not evaluated instead of placeholder zeros', () => {
  const html = renderRecords([
    {
      ...emptyRecord,
      source_format: 'ogxm',
      engine: 'Unknown',
      evaluated: false,
      engine_version: null,
      depth: null,
      depth_plies: null,
      method: 'unknown',
      raw: {
        analysis_id: 'b86cd1b0-4d12-80e6-86ba-511a59c022f0',
        currency: null,
        best_equity: 0,
        equity_loss: 0,
      },
    },
  ]);
  expect(html).toContain('Not evaluated');
  expect(html).not.toContain('Recorded OGXM values');
  expect(html).not.toContain('0.000');
});

test('OGXM without candidates displays the recorded zero values and the unknown unit', () => {
  const html = renderRecords([
    {
      ...emptyRecord,
      source_format: 'ogxm',
      raw: { currency: null, best_equity: 0, equity_loss: 0 },
    },
  ]);
  expect(html).toContain('Recorded OGXM values');
  expect(html).toContain('Unit unknown');
  expect(html).toMatch(/Best equity<\/th><td>0\.000<\/td>/);
  expect(html).toMatch(/Loss<\/th><td>0\.000<\/td>/);
  expect(html).toContain('No candidate values recorded.');
  expect(html).not.toContain('candidate-comparison-table');
});

test('OGXM MWC loss is shown in percentage points and missing best equity is omitted', () => {
  const html = renderRecords([
    {
      ...emptyRecord,
      source_format: 'ogxm',
      raw: { currency: 3, best_equity: null, equity_loss: 0.012345 },
    },
  ]);
  expect(html).toMatch(/Loss<\/th><td>1\.2345 pp<\/td>/);
  expect(html).not.toContain('Best equity</th>');
});

test('OGXM MWC is formatted in percent and missing loss is omitted', () => {
  const html = renderRecords([
    {
      ...emptyRecord,
      source_format: 'ogxm',
      raw: { currency: 2, best_equity: 0.61234, equity_loss: null },
    },
  ]);
  expect(html).toMatch(/Best equity<\/th><td>61\.234%<\/td>/);
  expect(html).not.toContain('Loss</th>');
});

test('an OGXM record without finite summary values keeps the empty state', () => {
  const html = renderRecords([
    {
      ...emptyRecord,
      source_format: 'ogxm',
      raw: { best_equity: null, equity_loss: '0' },
    },
  ]);
  expect(html).not.toContain('Recorded OGXM values');
  expect(html).toContain('No candidate values recorded.');
});

test('an imported OGXM take labels the doubler as the perspective of the recorded MWC', () => {
  const html = renderRecords([
    {
      ...emptyRecord,
      source_format: 'ogxm',
      kind: 'cube',
      player: 1,
      played: 'Take',
      cube: {
        equity_kind: 'mwc',
        equity_player: 0,
        no_double: 0.481127,
        double_take: 0.493084,
        double_pass: 0.5,
      },
    },
  ]);
  expect(html).toContain('Cube response without dice');
  expect(html).toContain('Values from the doubler’s perspective');
  expect(html).toContain('49.308%');
});

test.each([
  { actor: 0, equityPlayer: 0, played: 'Double', name: 'Raphaël Ginoulhac' },
  { actor: 1, equityPlayer: 1, played: 'Double', name: 'Guilaume Gallé' },
  { actor: 0, equityPlayer: 0, played: 'No double', name: 'Raphaël Ginoulhac' },
  { actor: 1, equityPlayer: 1, played: 'No double', name: 'Guilaume Gallé' },
  { actor: 1, equityPlayer: 0, played: 'Take', name: 'Raphaël Ginoulhac' },
  { actor: 0, equityPlayer: 1, played: 'Take', name: 'Guilaume Gallé' },
  { actor: 1, equityPlayer: 0, played: 'Drop', name: 'Raphaël Ginoulhac' },
])(
  'OGXM $played names $name as the source perspective',
  ({ actor, equityPlayer, played, name }) => {
    const html = renderRecords(
      [
        {
          ...emptyRecord,
          source_format: 'ogxm',
          kind: 'cube',
          player: actor,
          played,
          cube: {
            equity_kind: 'mwc',
            equity_player: equityPlayer,
            double_take: 0.493084,
          },
        },
      ],
      'cube',
      { white: 'Raphaël Ginoulhac', black: 'Guilaume Gallé' },
    );
    expect(html).toContain(`Values from ${name}’s perspective`);
    expect(html).toContain('49.308%');
  },
);

test('an imported XG offer uses the offerer when the source specifies the doubler', () => {
  const html = renderRecords(
    [
      {
        ...emptyRecord,
        source_format: 'xg',
        kind: 'cube',
        player: 1,
        played: 'Double',
        cube: {
          equity_kind: 'cubeful',
          perspective: 'doubler',
          double_take: 0.1,
        },
      },
    ],
    'cube',
    { white: 'Raphaël Ginoulhac', black: 'Guilaume Gallé' },
  );
  expect(html).toContain('Values from Guilaume Gallé’s perspective');
});
