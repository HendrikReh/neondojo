import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import MatchAnalysisPanel from './MatchAnalysisPanel';
import { I18nProvider } from './i18n';
import type { Job, Match } from './api';

vi.mock('./useMatchAnalysis', () => ({
  useMatchAnalysis: () => ({
    job: {
      id: 'failed-native-job',
      status: 'failed',
      completed_games: 0,
      total_games: 2,
      progress: 0,
      plies: 2,
      error: 'native failure',
    } as Job,
    error: null,
    submitting: false,
    run: () => {},
  }),
}));
beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const match = { id: 'match', white: 'Alice', black: 'Bob' } as Match;
const render = (showStatistics = false, overview = false) =>
  renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchAnalysisPanel, {
        match,
        gameNo: 3,
        onPosition: () => {},
        showStatistics,
        showResults: overview || !showStatistics,
        overview,
      }),
    }),
  );
it('shows explicit severity and game filters and a reset action', () => {
  const html = render();
  expect(html).toContain('aria-label="Filter by severity"');
  expect(html).toContain('value="none"');
  expect(html).toContain('value="very_bad"');
  expect(html).toContain('aria-label="Filter by game"');
  expect(html).toContain('Reset filters');
  expect(html).toContain('<option value="3" selected="">Game 3</option>');
  expect(html).not.toContain('All games in this analysis');
});
it('starts the focused error list with only actual inaccuracies and errors', () => {
  expect(render()).toMatch(/type="checkbox"[^>]*checked/);
});
it('keeps failed native-job progress and errors available on the statistics tab', () => {
  const html = render(true);
  const stats = html.slice(html.indexOf('id="inspector-panel-statistics"'));
  expect(stats).toContain('native failure');
  expect(stats).toContain('analysis failed');
  expect(stats).toContain('role="status"');
});
it('shows progress once while overview exposes both results and statistics', () => {
  const html = render(true, true);
  expect(html).toContain('overview-errors');
  expect(html).toContain('overview-statistics');
  expect(html.match(/class="match-analysis-progress"/g)).toHaveLength(1);
  expect(html.match(/native failure/g)).toHaveLength(1);
  expect(html).toContain('aria-label="Filter by severity"');
});
