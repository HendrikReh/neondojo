import { entryForJob } from '../analysis-catalog-test-fixtures';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { I18nProvider } from '../../../shared/i18n/i18n';
import { placeholders as args } from '../../../shared/i18n/i18n-core';
import type { Job, MatchAnalysisSummary } from '../../../shared/http/api';
import {
  buildStatisticsFilter,
  decisionFilterQuery,
  nativePerformanceRating,
  statisticsRevision,
} from './match-statistics-api';
import MatchStatisticsView from './MatchStatisticsView';
import {
  statisticsMessages,
  statisticsText,
} from '../../../shared/review/review-statistics-messages';

beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const summary: MatchAnalysisSummary = {
  analyzed_moves: 4,
  total_moves: 8,
  completed_games: 1,
  total_games: 2,
  engine: 'GNU Backgammon 1.08',
  plies: 2,
  players: [0, 1].map((player) => ({
    player,
    checker_decisions: 2,
    cube_decisions: 1,
    checker_equity_loss: 0.04,
    cube_equity_loss: 0.08,
    doubtful: 1,
    bad: 1,
    very_bad: 0,
    forced_moves: 1,
    unavailable: 0,
  })),
  performance_rating: {
    method: 'gnubg_emg_500_v1',
    profile: 'GNU Backgammon 1.08; 2-ply cubeful deterministic',
    games: 1,
    players: [0, 1].map((player) => ({
      player,
      value: 20,
      normalized_equity_loss: 0.12,
      unforced_checker_moves: 2,
      close_cube_decisions: 1,
    })),
  },
};
const job = {
  id: 'native-job',
  kind: 'match',
  completed_games: 1,
  total_games: 2,
  updated_at: '2026-11-08T10:00:00Z',
  status: 'running',
  plies: 2,
} as Job;

it('metric actions replace visible filters and intersect player, severity and source game', () => {
  expect(
    buildStatisticsFilter({ metric: 'very_bad', player: 1, gameNo: 7 }),
  ).toEqual({
    player: '1',
    kind: '',
    severity: 'very_bad',
    game_no: 7,
    errors_only: true,
  });
  expect(buildStatisticsFilter({ metric: 'cube', player: 0 })).toEqual({
    player: '0',
    kind: 'cube',
    severity: '',
    game_no: undefined,
    errors_only: false,
  });
  expect(
    buildStatisticsFilter({ metric: 'inaccuracies', player: 0 }).errors_only,
  ).toBe(true);
  expect(buildStatisticsFilter({ metric: 'loss', player: 1 }).errors_only).toBe(
    false,
  );
  expect(buildStatisticsFilter({ metric: 'pr', player: 1 }).kind).toBe('');
});

it('decision requests preserve every filter including exact no-error severity', () => {
  const query = new URLSearchParams(
    decisionFilterQuery({
      player: '1',
      kind: 'checker',
      severity: 'none',
      game_no: 9,
      errors_only: false,
    }),
  );
  expect(Object.fromEntries(query)).toEqual({
    errors_only: 'false',
    player: '1',
    kind: 'checker',
    severity: 'none',
    game_no: '9',
  });
  expect(
    decisionFilterQuery({
      player: '',
      kind: '',
      severity: '',
      errors_only: false,
    }),
  ).toBe('errors_only=false');
});

it('native PR stays missing for legacy, unknown-method and denominator-free data', () => {
  expect(
    nativePerformanceRating({ ...summary, performance_rating: null }, 0),
  ).toBeUndefined();
  expect(
    nativePerformanceRating(
      {
        ...summary,
        performance_rating: { ...summary.performance_rating!, method: 'old' },
      },
      0,
    ),
  ).toBeUndefined();
  expect(
    nativePerformanceRating(
      {
        ...summary,
        performance_rating: {
          ...summary.performance_rating!,
          profile: 'other source',
        },
      },
      0,
    ),
  ).toBeUndefined();
  expect(
    nativePerformanceRating(
      {
        ...summary,
        performance_rating: { ...summary.performance_rating!, games: 2 },
      },
      0,
    ),
  ).toBeUndefined();
  expect(
    nativePerformanceRating(
      {
        ...summary,
        performance_rating: {
          ...summary.performance_rating!,
          players: [
            {
              ...summary.performance_rating!.players[0],
              value: null,
              unforced_checker_moves: 0,
              close_cube_decisions: 0,
            },
          ],
        },
      },
      0,
    ),
  ).toBeUndefined();
  expect(nativePerformanceRating(summary, 1)).toBe(20);
});

it('statistics revisions fence new jobs and completed-game checkpoints', () => {
  expect(statisticsRevision(job)).not.toBe(
    statisticsRevision({ ...job, completed_games: 2 }),
  );
  expect(statisticsRevision(job)).not.toBe(
    statisticsRevision({ ...job, id: 'other' }),
  );
  expect(statisticsRevision(job)).not.toBe(
    statisticsRevision({ ...job, status: 'failed' }),
  );
});

it('renders native source, partial coverage, precise games and missing legacy game PR', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchStatisticsView, {
        analysisEntry: entryForJob(job),
        names: ['Alice', 'Bob'],
        statistics: {
          analysis_id: `job:${job.id}`,
          summary,
          games: [
            {
              game_no: 7,
              white_score: 3,
              black_score: 4,
              crawford: true,
              analyzed: true,
              summary: {
                ...summary,
                completed_games: 1,
                total_games: 1,
                performance_rating: null,
              },
            },
            {
              game_no: 8,
              white_score: 4,
              black_score: 4,
              crawford: false,
              analyzed: false,
              summary: null,
            },
          ],
        },
        onSelect: () => {},
        onGame: () => {},
      }),
    }),
  );
  expect(html).toContain('id="inspector-panel-statistics"');
  expect(html.match(/<table/g)).toHaveLength(1);
  expect(html).toContain('aria-labelledby="inspector-tab-statistics"');
  expect(html).toContain('GNU Backgammon 1.08');
  expect(html).toContain('1 of 2 games');
  expect(html).toContain('3:4');
  expect(html).toContain('Crawford');
  expect(html).toContain('Open game 7');
  expect(html).toContain('Not analyzed');
  expect(html).toContain('Unavailable');
  expect(html).not.toContain('>0.000<');
});

it('provides every statistics message and parameter in both languages', () => {
  const source = statisticsMessages.en;
  for (const catalog of Object.values(statisticsMessages)) {
    expect(Object.keys(catalog).sort()).toEqual(Object.keys(source).sort());
    for (const key of Object.keys(source) as Array<keyof typeof source>) {
      expect(catalog[key]).not.toBe('');
      expect(args(catalog[key]), key).toEqual(args(source[key]));
    }
  }
  expect(statisticsText('de', 'game', { game: 7 })).toContain('7');
});

it('single-game native summaries carry their source game in metric actions', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchStatisticsView, {
        analysisEntry: entryForJob({ ...job, game_no: 7 }, 7),
        names: ['Alice', 'Bob'],
        onSelect: () => {},
        statistics: {
          analysis_id: `job:${job.id}`,
          summary,
          games: [],
          error_breakdown: [
            {
              player: 0,
              checker: { doubtful: 0, bad: 0, blunder: 0, very_bad: 1 },
              cube: { doubtful: 0, bad: 0, blunder: 0, very_bad: 0 },
            },
          ],
        },
      }),
    }),
  );
  expect(html).toContain(
    statisticsText('en', 'metricGameAction', {
      game: 7,
      player: 'Alice',
      metric: 'Large Blunder',
    }),
  );
  expect(html).not.toContain(
    statisticsText('en', 'metricAction', {
      player: 'Alice',
      metric: 'Large Blunder',
    }),
  );
});

it('typed error actions intersect kind, severity, player and source game', () => {
  expect(
    buildStatisticsFilter({
      metric: 'bad',
      kind: 'cube',
      player: 1,
      gameNo: 7,
    }),
  ).toEqual({
    player: '1',
    kind: 'cube',
    severity: 'bad',
    game_no: 7,
    errors_only: true,
  });
  expect(
    buildStatisticsFilter({
      metric: 'inaccuracies',
      kind: 'checker',
      player: 0,
    }),
  ).toEqual({
    player: '0',
    kind: 'checker',
    severity: '',
    game_no: undefined,
    errors_only: true,
  });
});

it('renders a compact typed matrix with actionable zeros and preserves total metric labels', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchStatisticsView, {
        analysisEntry: entryForJob(job),
        names: ['Alice', 'Bob'],
        onSelect: () => {},
        statistics: {
          analysis_id: `job:${job.id}`,
          summary,
          games: [],
          error_breakdown: [0, 1].map((player) => ({
            player,
            checker: { blunder: 0, doubtful: 1, bad: 0, very_bad: 0 },
            cube: { blunder: 0, doubtful: 0, bad: 1, very_bad: 0 },
          })),
        },
      }),
    }),
  );
  expect(html).toContain('class="statistics-error-matrix"');
  expect(html).toContain('data-statistics-kind="checker"');
  expect(html).toContain('data-statistics-kind="cube"');
  expect(html).toContain('Show Alice: Error');
  expect(html).toContain('Show Alice: Cube · Error');
  expect(html).toMatch(
    /data-statistics-kind="checker"[^>]*aria-label="Show Alice: Checker · Error"[^>]*><strong>0<\/strong>/,
  );
});

it('typed totals equal checker plus cube even when legacy summary severity counts differ', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(MatchStatisticsView, {
        analysisEntry: entryForJob(job),
        names: ['Alice', 'Bob'],
        onSelect: () => {},
        statistics: {
          analysis_id: `job:${job.id}`,
          summary: {
            ...summary,
            players: summary.players.map((p) => ({ ...p, bad: 9 })),
          },
          games: [],
          error_breakdown: [0, 1].map((player) => ({
            player,
            checker: { blunder: 0, doubtful: 0, bad: 1, very_bad: 0 },
            cube: { blunder: 0, doubtful: 0, bad: 2, very_bad: 0 },
          })),
        },
      }),
    }),
  );
  expect(html).toMatch(
    /aria-label="Show Alice: Error"[^>]*><strong>3<\/strong>/,
  );
  expect(html).not.toMatch(
    /aria-label="Show Alice: Error"[^>]*><strong>9<\/strong>/,
  );
});
