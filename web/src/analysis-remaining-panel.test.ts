import { catalogFixture, entryForJob } from './analysis-catalog-test-fixtures';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import MatchAnalysisPanel from './MatchAnalysisPanel';
import { I18nProvider } from './i18n';
import type { Job, Match } from './api';
const state = vi.hoisted(() => ({ status: 'running' }));
vi.mock('./useMatchAnalysis', () => ({
  useMatchAnalysis: () => ({
    job: {
      id: 'timed-job',
      kind: 'match',
      status: state.status,
      completed_games: 0,
      total_games: 1,
      progress: 0,
      plies: 2,
      attempts: 1,
      updated_at: '2026-11-10T12:00:00Z',
    } as Job,
    error: null,
    submitting: false,
    run: () => {},
  }),
}));
beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const render = () =>
  renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchAnalysisPanel, {
        match: {
          id: 'source',
          white: 'Alice',
          black: 'Bob',
          source_available: true,
          position_count: 100,
        } as Match,
        gameNo: 1,
        catalog: catalogFixture(
          entryForJob(
            {
              id: 'timed-job',
              kind: 'match',
              status: state.status,
              completed_games: 0,
              total_games: 1,
              progress: 0,
              plies: 2,
              attempts: 1,
              updated_at: '2026-11-10T12:00:00Z',
            } as Job,
            1,
          ),
        ),
      }),
    }),
  );
it.each(['running', 'queued'])(
  'adds remaining time to actual %s match progress',
  (status) => {
    state.status = status;
    const html = render();
    expect(html).toContain('analysis-remaining-time');
    expect(html).toContain(
      status === 'running'
        ? 'Calculating time remaining'
        : 'Waiting for analysis to start',
    );
    expect(html).not.toContain('class="analysis-time-estimate"');
  },
);
it.each(['completed', 'failed'])(
  'hides the active countdown after %s',
  (status) => {
    state.status = status;
    expect(render()).not.toContain('analysis-remaining-time');
  },
);
