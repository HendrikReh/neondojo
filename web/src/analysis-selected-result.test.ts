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

it('shows the ranked candidates from the selected game analysis', () => {
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
    checker_candidates: [
      {
        rank: 1,
        notation: '24/18 13/10',
        equity: 0.2,
        probabilities: [0.6, 0.2, 0, 0.4, 0.1, 0],
        plies: 3,
      },
      {
        rank: 2,
        notation: '24/15',
        equity: 0.1,
        probabilities: [0.55, 0.2, 0, 0.45, 0.1, 0],
        plies: 1,
      },
      {
        rank: 3,
        notation: '13/7 6/3',
        equity: -0.1,
        probabilities: [0.45, 0.1, 0, 0.55, 0.1, 0],
        plies: 3,
      },
    ],
  };
  const result: PositionAnalysisResult = {
    analysis_id: 'job:game',
    position_id: 'opening',
    engine: 'GNU Backgammon',
    depth: '3-ply',
    records: [],
    decision,
  };
  const html = render(result);
  expect(html).toContain('candidate-comparison-table');
  expect(html).toContain('24/15');
  expect(html).toContain('1-ply');
  expect(html).toContain('data-candidate-played="true"');

  const { checker_candidates: _, ...legacy } = decision;
  expect(render({ ...result, decision: legacy })).toContain('Best decision');
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
        positionJobs: {
          jobs: {
            analysis: {
              id: 'new',
              status: 'completed',
              kind: 'analysis',
              result: null,
            } as import('./api').Job,
          },
          run: async () => {},
          refresh: () => {},
          removeDice: async () => {},
          submitting: false,
          restoring: false,
          error: undefined,
          failure: undefined,
        },
        disabled: false,
        resultsVisible: true,
        stats: null,
      }),
    }),
  );
  expect(html).toContain('Show calculated result');
  expect(html).not.toContain('analysis-source-tabs');
});
