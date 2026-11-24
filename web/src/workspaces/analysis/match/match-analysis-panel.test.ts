import { catalogFixture, entryForJob } from '../analysis-catalog-test-fixtures';
import { createElement, Fragment } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import MatchAnalysisHeader from './MatchAnalysisHeader';
import MatchAnalysisPanel from './MatchAnalysisPanel';
import { useMatchReview } from './useMatchReview';
import { I18nProvider } from '../../../shared/i18n/i18n';
import type { Job, Match } from '../../../shared/http/api';

vi.mock('./useMatchAnalysis', () => ({
  useMatchAnalysis: () => ({
    job: {
      id: 'failed-native-job',
      kind: 'match',
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
type Catalog = ReturnType<typeof catalogFixture>;
function Analysis({
  catalog,
  showStatistics,
}: {
  catalog: Catalog;
  showStatistics: boolean;
}) {
  const review = useMatchReview({
    match,
    catalog,
    gameNo: 3,
    reviewKind: 'cube',
    showResults: !showStatistics,
    checkerFiltersOpen: false,
    onDecision: () => {},
    onShowReview: () => {},
    onGame: () => {},
  });
  return createElement(
    Fragment,
    null,
    createElement(MatchAnalysisHeader, {
      match,
      gameNo: 3,
      defaultEngine: 'gnubg',
      defaultPlies: 3,
      configured: true,
      catalog,
    }),
    createElement(MatchAnalysisPanel, {
      review,
      match,
      gameNo: 3,
      catalog,
      showResults: !showStatistics,
      showStatistics,
    }),
  );
}
const render = (showStatistics = false) =>
  renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(Analysis, {
        catalog: catalogFixture(
          entryForJob(
            {
              id: 'failed-native-job',
              kind: 'match',
              status: 'failed',
              completed_games: 0,
              total_games: 2,
              progress: 0,
              plies: 2,
              error: 'native failure',
            } as Job,
            3,
          ),
        ),
        showStatistics,
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
it('starts the focused error review with only actual inaccuracies and errors', () => {
  expect(render()).toMatch(/type="checkbox"[^>]*checked/);
});
it('keeps failed native-job progress and errors in the header on the statistics tab', () => {
  const html = render(true);
  const header = html.slice(0, html.indexOf('id="inspector-panel-errors"'));
  expect(header).toContain('native failure');
  expect(header).toContain('analysis failed');
  expect(header).toContain('role="status"');
  expect(html.match(/native failure/g)).toHaveLength(1);
});
it('shows progress once when only the error review is visible', () => {
  const html = render();
  expect(html).toMatch(/id="inspector-panel-statistics"[^>]* hidden=""/);
  expect(html.match(/class="match-analysis-progress"/g)).toHaveLength(1);
  expect(html.match(/native failure/g)).toHaveLength(1);
  expect(html).toContain('aria-label="Filter by severity"');
});

it('does not label a completed position calculation as a completed match or zero-game coverage', () => {
  const positionJob = {
    id: 'position-analysis',
    kind: 'analysis',
    status: 'completed',
    completed_games: 0,
    total_games: 0,
    progress: 100,
    plies: 2,
  } as Job;
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(Analysis, {
        showStatistics: true,
        catalog: catalogFixture({
          ...entryForJob(positionJob),
          scope: 'position',
          position_id: 'position',
        }),
      }),
    }),
  );
  expect(html).not.toContain('class="match-analysis-progress"');
  expect(html).not.toContain('0 of 0 games');
  expect(html).toContain('GNU Backgammon');
});
