import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { beforeEach, afterEach, expect, it, vi } from 'vitest';
import { I18nProvider } from './i18n';
import { SelectedAnalysisResult } from './AnalysisPanel';
import type { PositionAnalysisResult } from './analysis-catalog-api';
beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const render = (result?: PositionAnalysisResult | null) =>
  renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(SelectedAnalysisResult, {
        result,
        selected: true,
        alternatives: true,
      }),
    }),
  );
it('explains missing coverage in the chosen analysis instead of showing another source', () => {
  expect(render(null)).toContain(
    'This position has no result in the selected analysis.',
  );
  expect(render(null)).toContain('Other saved analyses can be selected above.');
});

it('shows the played and best decisions from the selected game analysis', () => {
  const decision = {
    decision_no: 1,
    checker_no: 1,
    player: 0,
    kind: 'checker',
    played: '13/7 6/3',
    best: '24/18 13/10',
    played_equity: -0.1,
    best_equity: 0.2,
    equity_loss: 0.3,
    severity: 'very_bad',
    dice: [6, 3] as [number, number],
    evaluation_status: 'evaluated',
  };
  const result: PositionAnalysisResult = {
    analysis_id: 'job:game',
    position_id: 'opening',
    engine: 'GNU Backgammon',
    depth: '2-ply',
    records: [],
    decision,
  };
  const html = render(result);
  expect(html).toContain('Best decision');
  expect(html).toContain('24/18 13/10');
  expect(html).toContain('13/7 6/3');
  expect(html).toContain('0.300');
});

it('offers an explicit action for a completed calculation while keeping the selected source', async () => {
  const { default: AnalysisPanel } = await import('./AnalysisPanel');
  const { catalogFixture } = await import('./analysis-catalog-test-fixtures');
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(AnalysisPanel, {
        catalog: {
          ...catalogFixture(),
          analysisId: 'imported:match',
          selectedId: 'imported:match',
        },
        jobs: {
          analysis: {
            id: 'new',
            status: 'completed',
            kind: 'analysis',
            result: null,
          } as import('./api').Job,
        },
        disabled: false,
        stats: null,
        onAnalyze: () => {},
      }),
    }),
  );
  expect(html).toContain('Show calculated result');
  expect(html).not.toContain('analysis-source-tabs');
});
