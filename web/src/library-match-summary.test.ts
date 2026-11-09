import { expect, it } from 'vitest';
import { createMatchSummaryFormatter } from './library-match-summary';
import type { LibraryMatch } from './library-api';

const match: LibraryMatch = {
  source_available: true,
  id: '00000000-0000-0000-0000-000000000007',
  match_no: 7,
  database_id: '00000000-0000-0000-0000-000000000001',
  filename: 'source.xg',
  display_name: 'Final training',
  format: 'xg',
  white: 'Alice',
  black: 'Bob',
  match_length: 11,
  position_count: 460,
  game_count: 10,
  created_at: '2026-11-09T12:00:00Z',
  played_on: '2026-11-08',
  tournament: 'Autumn Open',
  round: 'Final',
  place: 'Berlin',
  result_state: 'completed',
  white_score: 11,
  black_score: 9,
  white_pr: 2,
  black_pr: 4.75,
  pr_method: 'gnubg_emg_500_v1',
  pr_profile: 'GNUbg; 2-ply cubeful deterministic',
  analysis_status: 'completed',
  tags: ['Study', 'Cube'],
  comment: 'Review\n  the cube decision.',
  version: 0,
};

it('summarizes identity, players, score, analysis, context and notes in one text', () => {
  const summary = createMatchSummaryFormatter('en')(match);
  for (const text of [
    'ID: 7 · Final training',
    'Alice — Bob',
    'Points: 11',
    'Result: 11–9',
    'White 2.00',
    'Black 4.75',
    'Games: 10',
    'Positions: 460',
    'Analysis: Completed',
    '2-ply',
    'Tournament: Autumn Open',
    'Round: Final',
    'Location: Berlin',
    'Tags: Study, Cube',
    'Comment: Review the cube decision.',
  ])
    expect(summary).toContain(text);
  expect(summary).not.toContain('source.xg');
});
it('uses readable moneygame context and n/a without fabricating scores or PR', () => {
  const summary = createMatchSummaryFormatter('en')({
    ...match,
    match_length: 0,
    display_name: match.filename,
    white_score: null,
    black_score: null,
    white_pr: null,
    black_pr: null,
    pr_profile: null,
    analysis_status: null,
  });
  expect(summary).toContain('Points: Moneygame');
  expect(summary).toContain('Result: n/a');
  expect(summary).toContain('Analysis: Not analysed');
  expect(summary).not.toContain('PR:');
  expect(summary).not.toContain(match.filename);
});
it.each([
  ['en', 'Positions', 'Imported'],
  ['de', 'Positionen', 'Importiert'],
  ['es', 'Posiciones', 'Importado'],
  ['fr', 'Positions', 'Importée'],
  ['pl', 'Pozycje', 'Zaimportowano'],
  ['it', 'Posizioni', 'Importata'],
] as const)(
  'uses %s labels and analysis status',
  (locale, positions, status) => {
    const summary = createMatchSummaryFormatter(locale)({
      ...match,
      analysis_status: 'imported',
    });
    expect(summary).toContain(`${positions}: 460`);
    expect(summary).toContain(status);
  },
);
it('keeps long comments and tag collections bounded and plain text', () => {
  const summary = createMatchSummaryFormatter('en')({
    ...match,
    tags: Array.from({ length: 30 }, (_, i) => `Tag ${i}`),
    comment: '🧠'.repeat(10000),
  });
  expect(summary.length).toBeLessThan(1800);
  expect(summary).toContain('Tag 0');
  expect(summary).not.toContain('Tag 29');
  expect(summary).toContain('…');
  expect(summary).not.toContain('\uFFFD');
});
