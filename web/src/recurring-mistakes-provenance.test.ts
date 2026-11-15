import { expect, test } from 'vitest';
import {
  mistakeProvenance,
  mistakeReferenceKey,
} from './recurring-mistakes-provenance';

const text = (
  key:
    | 'nativeSource'
    | 'xgSource'
    | 'bgblitzSource'
    | 'sgfSource'
    | 'unknownDepth',
) =>
  ({
    nativeSource: 'Local analysis',
    xgSource: 'Imported XG',
    bgblitzSource: 'Imported BGBlitz',
    sgfSource: 'Imported GNUbg',
    unknownDepth: 'Unknown depth',
  })[key];

test('original XG search and rollout labels take precedence over any ply number', () => {
  expect(
    mistakeProvenance(
      {
        engine: 'eXtreme Gammon',
        plies: null,
        depth: 'XGRoller++',
        source: 'xg',
        method: 'rollout',
      },
      text,
    ),
  ).toBe('Imported XG · eXtreme Gammon · XGRoller++ · rollout');
});
test('unknown imported depth does not become a zero-ply evaluation', () => {
  expect(
    mistakeProvenance(
      { engine: 'BGBlitz source', plies: null, source: 'bgblitz' },
      text,
    ),
  ).toBe('Imported BGBlitz · BGBlitz source · Unknown depth');
  expect(mistakeProvenance({ engine: 'GNUbg', plies: 0 }, text)).toBe(
    'GNUbg · 0 ply',
  );
});
test('different imported records with the same game and move reset question state', () => {
  const common = { game_no: 1, decision_no: 2 };
  expect(
    mistakeReferenceKey({ ...common, imported_analysis_id: 'first' }),
  ).not.toBe(
    mistakeReferenceKey({ ...common, imported_analysis_id: 'second' }),
  );
  expect(mistakeReferenceKey({ ...common, job_id: 'first' })).not.toBe(
    mistakeReferenceKey({ ...common, imported_analysis_id: 'first' }),
  );
});
