import { catalogFixture } from '../../workspaces/analysis/analysis-catalog-test-fixtures';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { PositionAnalysisDepth } from '../../workspaces/analysis/AnalysisPanel';
import { ANALYSIS_PLIES } from './analysis-depth';
import MatchAnalysisPanel from '../../workspaces/analysis/match/MatchAnalysisPanel';
import { I18nProvider } from '../i18n/i18n';
import type { Match } from '../api/api';

vi.mock('../../workspaces/analysis/match/useMatchAnalysis', () => ({
  useMatchAnalysis: () => ({
    job: undefined,
    error: null,
    submitting: false,
    run: () => {},
  }),
}));
beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());

it('defaults position search to GNUbg 3-ply and names GNUbg’s own level', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(PositionAnalysisDepth, {
        engine: 'gnubg',
        supportedPlies: ANALYSIS_PLIES,
        onChange: () => {},
      }),
    }),
  );
  expect(html).not.toContain('value="0"');
  expect(html).toContain('value="1">1-ply (GNUbg 0-ply) · fast</option>');
  expect(html).toContain(
    'value="3" selected="">3-ply (GNUbg 2-ply) · more accurate</option>',
  );
  expect(html).toContain('value="5">5-ply (GNUbg 4-ply)</option>');
});
it('labels NeonGammon depths without a GNUbg level', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(PositionAnalysisDepth, {
        engine: 'bgsage',
        plies: 2,
        supportedPlies: [1, 2, 3, 4],
        onChange: () => {},
      }),
    }),
  );
  expect(html).toContain('value="2" selected="">2-ply</option>');
  expect(html).not.toContain('GNUbg');
  expect(html).toContain('value="5" disabled="">5-ply</option>');
});
it('defaults game/match analysis to GNUbg 3-ply and offers 4- and 5-ply', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchAnalysisPanel, {
        match: { id: 'match', white: 'Alice', black: 'Bob' } as Match,
        gameNo: 1,
        catalog: catalogFixture(),
      }),
    }),
  );
  expect(html).toContain(
    'value="3" selected="">3-ply (GNUbg 2-ply) · Thorough</option>',
  );
  expect(html).toContain('value="4">4-ply (GNUbg 3-ply)</option>');
  expect(html).toContain('value="5">5-ply (GNUbg 4-ply)</option>');
});
