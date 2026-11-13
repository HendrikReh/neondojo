import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Job, MatchAnalysisSummary } from './api';
import { I18nProvider } from './i18n';
import { MatchAnalysisComparisonResults } from './MatchAnalysisComparison';

beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());

function job(
  id: string,
  engine: 'gnubg' | 'bgsage',
  plies: number,
  loss: number,
  status: Job['status'] = 'completed',
): Job {
  const date = '2026-11-12T12:00:00Z';
  const result: MatchAnalysisSummary = {
    engine: 'test version',
    plies,
    total_games: 1,
    completed_games: 1,
    total_moves: 2,
    analyzed_moves: 2,
    performance_rating: {
      method: 'gnubg_emg_500_v1',
      profile: `test version; ${plies}-ply cubeful deterministic`,
      games: 1,
      players: [
        {
          player: 0,
          value: (500 * loss) / 3,
          normalized_equity_loss: loss,
          unforced_checker_moves: 2,
          close_cube_decisions: 1,
        },
      ],
    },
    players: [
      {
        player: 0,
        checker_decisions: 2,
        cube_decisions: 1,
        forced_moves: 0,
        unavailable: 0,
        doubtful: 0,
        bad: 0,
        very_bad: 0,
        checker_equity_loss: loss,
        cube_equity_loss: 0,
      },
    ],
  };
  return {
    id,
    kind: 'match',
    plies,
    status,
    result: status === 'completed' ? result : null,
    progress: status === 'completed' ? 100 : 0,
    completed_games: status === 'completed' ? 1 : 0,
    total_games: 1,
    attempts: 0,
    created_at: date,
    updated_at: date,
    engine_snapshot: { engine_id: engine, gnubg_path: 'gnubg' },
  };
}

function render(jobs: Job[]) {
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchAnalysisComparisonResults, {
        jobs,
        names: ['Alice', 'Bob'],
      }),
    }),
  );
}

it('aligns both providers and defaults to completed 3-ply even with more recent runs', () => {
  const html = render([
    job('recent-gnubg', 'gnubg', 4, 0.999),
    job('running-sage', 'bgsage', 3, 0, 'running'),
    job('gnubg-three', 'gnubg', 3, 0.124),
    job('sage-three', 'bgsage', 3, 0.456),
  ]);
  expect(html).toContain('<option value="gnubg-three" selected="">');
  expect(html).toContain('<option value="sage-three" selected="">');
  expect(html).toContain('GNUbg PR');
  expect(html).toContain('>0.124<');
  expect(html).toContain('>0.456<');
  expect(html).not.toContain('>0.999<');
  expect(html).toContain('>20.667<');
  expect(html).not.toContain('>76.000<');
});

it('shows a missing provider explicitly and preserves the available lower depth', () => {
  const html = render([job('sage-two', 'bgsage', 2, 0.456)]);
  expect(html).toContain('No full-match analysis');
  expect(html).toContain('<option value="sage-two" selected="">');
  expect(html).toContain('>0.456<');
});
