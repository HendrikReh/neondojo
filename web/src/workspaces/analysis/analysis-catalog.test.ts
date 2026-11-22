import { describe, expect, it } from 'vitest';
import {
  analysisCatalogPath,
  analysisResultPath,
  mergeCatalogEntries,
} from './analysis-catalog-api';
import type { AnalysisCatalogEntry } from './analysis-catalog-api';

describe('one selected analysis across consumers', () => {
  it('requests the exact selected result independently of calculation engine', () => {
    expect(analysisResultPath('position /1', 'imported:match')).toBe(
      '/positions/position%20%2F1/analysis-result?analysis_id=imported%3Amatch',
    );
    expect(analysisResultPath('position', 'job:older')).toContain(
      'analysis_id=job%3Aolder',
    );
  });
  it('keeps position and cursor encoded in bounded catalog queries', () => {
    const path = analysisCatalogPath('match', 'position', 'older cursor');
    expect(path).toBe(
      '/matches/match/analyses?limit=50&position_id=position&cursor=older+cursor',
    );
  });
  it('preserves all sources and updates overlapping pages without duplicates', () => {
    const native = {
      id: 'job:native',
      engine: 'GNUbg',
    } as AnalysisCatalogEntry;
    const imported = {
      id: 'imported:match',
      engine: 'eXtreme Gammon',
    } as AnalysisCatalogEntry;
    const updated = { ...native, complete: true };
    expect(mergeCatalogEntries([native], [updated, imported])).toEqual([
      updated,
      imported,
    ]);
  });
});
