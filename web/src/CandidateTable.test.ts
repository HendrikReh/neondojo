import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { I18nProvider } from './i18n';
import CandidateTable from './CandidateTable';

beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const candidates = [
  {
    rank: 1,
    notation: '8/5 6/5',
    equity: 0.2,
    equity_kind: 'cubeful',
    probabilities: [0.55],
    depth: '3-ply',
  },
  {
    rank: 2,
    notation: '13/10 6/5',
    equity: 0.1,
    equity_kind: 'cubeful',
    probabilities: { win: 0.52 },
    depth: '2-ply',
  },
];
function render(overrides: Partial<Parameters<typeof CandidateTable>[0]> = {}) {
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(CandidateTable, {
        candidates,
        source: 'GNUbg · 3-ply',
        depth: '3-ply',
        played: '13/10 6/5',
        onPreview: () => {},
        ...overrides,
      }),
    }),
  );
}
test('renders numerical losses and distinct best, played and preview markers', () => {
  const html = render({
    selectedPreview: { ...candidates[0], source: 'GNUbg · 3-ply' },
  });
  expect(html).toContain('Loss');
  expect(html).toContain('0.100');
  expect(html).toContain('55.0%');
  expect(html).toContain('52.0%');
  expect(html).toContain('data-candidate-best="true"');
  expect(html).toContain('data-candidate-played="true"');
  expect(html).toContain('data-preview-selected="true"');
  expect(html).toContain('Played');
  expect(html).toContain('Best');
  expect(html).toContain('Preview');
  expect(html).toContain('2-ply');
  expect(html).not.toContain('best-move');
});
test('shows unknown equity units explicitly without a fabricated loss', () => {
  const html = render({
    candidates: candidates.map((c) => ({ ...c, equity_kind: 'unknown' })),
  });
  expect(html).toContain('Unit unknown');
  expect(html).not.toContain('>0.100</td>');
  expect(html.match(/title="Difference[^"]*">—<\/td>/g)).toHaveLength(2);
});
test('MWC values use percentages and losses use percentage points', () => {
  const html = render({
    candidates: candidates.map((c) => ({ ...c, equity_kind: 'mwc' })),
  });
  expect(html).toContain('20.000%');
  expect(html).toContain('10.000 pp');
});

test('shows exclusive win outcomes for engine vectors and imported records', () => {
  const html = render({
    candidates: [
      { ...candidates[0], probabilities: [0.625, 0.25, 0.125] },
      {
        ...candidates[1],
        probabilities: { win: 0.5, win_gammon: 0.125, win_backgammon: 0 },
      },
    ],
  });
  expect(probabilityCells(html, 'win')).toEqual([
    ['62.5%', '37.5%', '12.5%', '12.5%'],
    ['50.0%', '37.5%', '12.5%', '0.0%'],
  ]);
});

test('shows only win outcomes supported by available and consistent values', () => {
  const html = render({
    candidates: [
      { ...candidates[0], probabilities: [0.625, 0.25] },
      {
        ...candidates[1],
        probabilities: { win: 0.5, win_gammon: 0.1, win_backgammon: 0.2 },
      },
    ],
  });
  expect(probabilityCells(html, 'win')).toEqual([
    ['62.5%', '37.5%', '—', '—'],
    ['50.0%', '—', '—', '—'],
  ]);
});

test('shows exact loss outcomes for engine vectors and imported records', () => {
  const html = render({
    candidates: [
      {
        ...candidates[0],
        probabilities: [0.625, 0.25, 0.125, 0.375, 0.125, 0.0625],
      },
      {
        ...candidates[1],
        probabilities: { win: 0.8, lose_gammon: 0.2, lose_backgammon: 0 },
      },
    ],
  });
  expect(probabilityCells(html, 'loss')).toEqual([
    ['37.5%', '25.0%', '6.3%', '6.3%'],
    ['20.0%', '0.0%', '20.0%', '0.0%'],
  ]);
});

test('keeps partial loss data distinct from recorded zeroes', () => {
  const html = render({
    candidates: [
      { ...candidates[0], probabilities: { lose: 0.375, lose_backgammon: 0 } },
      {
        ...candidates[1],
        probabilities: { win: 0.5, lose_gammon: 0.625, lose_backgammon: 0 },
      },
    ],
  });
  expect(probabilityCells(html, 'loss')).toEqual([
    ['37.5%', '—', '—', '0.0%'],
    ['50.0%', '—', '—', '—'],
  ]);
});

function probabilityCells(html: string, side: 'win' | 'loss') {
  const body = html.match(/<tbody>(.*?)<\/tbody>/s)?.[1] ?? '';
  return [...body.matchAll(/<tr[^>]*>(.*?)<\/tr>/gs)].map((row) =>
    [
      ...row[1].matchAll(
        new RegExp(`<span data-probability="${side}">([^<]*)</span>`, 'g'),
      ),
    ].map((cell) => cell[1]),
  );
}
