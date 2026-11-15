import { catalogFixture } from './analysis-catalog-test-fixtures';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { PositionAnalysisDepth } from './AnalysisPanel';
import { ANALYSIS_PLIES } from './analysis-depth';
import MatchAnalysisPanel from './MatchAnalysisPanel';
import { I18nProvider } from './i18n';
import type { Match } from './api';

vi.mock('./useMatchAnalysis', () => ({
  useMatchAnalysis: () => ({
    job: undefined,
    error: null,
    submitting: false,
    run: () => {},
  }),
}));
beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());

it('defaults position search to two plies and offers three and four plies', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(PositionAnalysisDepth, {
        supportedPlies: ANALYSIS_PLIES,
        onChange: () => {},
      }),
    }),
  );
  expect(html).toContain('value="2" selected=""');
  expect(html).toContain('value="3">3-ply</option>');
  expect(html).toContain('value="4">4-ply</option>');
});
it('defaults game/match analysis to two plies and offers three and four plies', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchAnalysisPanel, {
        match: { id: 'match', white: 'Alice', black: 'Bob' } as Match,
        gameNo: 1,
        catalog: catalogFixture(),
      }),
    }),
  );
  expect(html).toContain('value="2" selected=""');
  expect(html).toContain('value="3">3-ply</option>');
  expect(html).toContain('value="4">4-ply</option>');
});
