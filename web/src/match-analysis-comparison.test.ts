import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { I18nProvider } from './i18n';
import { MatchAnalysisComparisonResults } from './MatchAnalysisComparison';
import type {
  AnalysisCatalogEntry,
  MatchAnalysisStatistics,
} from './analysis-catalog-api';
import { entryForJob } from './analysis-catalog-test-fixtures';
import type { Job } from './api';
beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const native = entryForJob({
  id: 'native',
  plies: 4,
  status: 'completed',
  engine_snapshot: {
    engine_id: 'gnubg',
    gnubg_path: 'gnubg',
    engine_version: 'GNUbg',
  },
} as Job);
const imported: AnalysisCatalogEntry = {
  ...native,
  id: 'imported:match',
  source: 'imported',
  engine: 'eXtreme Gammon',
  depth: 'XGRoller++',
  job: null,
  complete: false,
};
const sage = { ...native, id: 'job:sage', engine: 'NeonGammon', depth: '3-ply' };
function statistics(
  entry: AnalysisCatalogEntry,
  loss: number,
): MatchAnalysisStatistics {
  return {
    analysis_id: entry.id,
    games: [],
    summary: {
      engine: entry.engine,
      plies: entry.source === 'native' ? 4 : null,
      completed_games: 1,
      total_games: 1,
      analyzed_moves: 2,
      total_moves: 2,
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
      performance_rating:
        entry.id === native.id
          ? {
              method: 'gnubg_emg_500_v1',
              profile: 'GNUbg; 4-ply cubeful deterministic',
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
            }
          : null,
    },
  };
}
function render(
  entries = [native, imported, sage],
  selection?: string[],
  saved = {
    [native.id]: statistics(native, 0.124),
    [imported.id]: statistics(imported, 0.456),
    [sage.id]: statistics(sage, 0.999),
  },
) {
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchAnalysisComparisonResults, {
        entries,
        statistics: saved,
        names: ['Alice', 'Bob'],
        selection,
      }),
    }),
  );
}
it('compares imported and native analyses with all saved entries available in either column', () => {
  const html = render();
  expect(html).toContain('<option value="job:native" selected="">');
  expect(html).toContain('<option value="imported:match" selected="">');
  expect(html).toContain('XGRoller++');
  expect(html).toContain('NeonGammon');
  expect(html).toContain('>0.124<');
  expect(html).toContain('>0.456<');
  expect(html).not.toContain('>0.999<');
  expect(html).toContain('>20.7<');
  expect(html).not.toContain('>76.000<');
});
it('respects explicit column choices without preferring engine names or ply levels', () => {
  const html = render(undefined, [sage.id, native.id]);
  expect(html).toContain('<option value="job:sage" selected="">');
  expect(html).toContain('>0.999<');
  expect(html).not.toContain('>0.456<');
});
it('keeps mismatched and missing statistics unavailable', () => {
  const html = render([imported], undefined, {
    [imported.id]: statistics(native, 0.999),
  });
  expect(html).toContain('No saved analysis is available.');
  expect(html).not.toContain('>0.999<');
});
