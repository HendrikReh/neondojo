import { errorThresholdLabels } from '../../shared/review/review-statistics-messages';
import { describe, expect, it } from 'vitest';
import { configurationSummaryRows } from './ProfileSummary';
import { defaultConfiguration } from '../configuration/configuration-model';
import {
  profileFieldKey,
  profileMessageKeys,
  profileText,
} from './profile-messages';
describe('profile summary', () => {
  it('describes every configured field without revision or runtime metadata', () => {
    const rows = configurationSummaryRows(defaultConfiguration);
    expect(rows.some((row) => row.path === 'analysis.plies')).toBe(true);
    expect(rows.some((row) => row.path.includes('schema_version'))).toBe(false);
    for (const row of rows)
      expect(
        row.path.startsWith('analysis.error_thresholds.')
          ? Object.hasOwn(errorThresholdLabels, row.path.split('.').at(-1)!)
          : !!profileFieldKey(row.path),
        row.path,
      ).toBe(true);
    expect(new Set(rows.map((row) => row.path.split('.')[0])).size).toBe(10);
  });
  it('supplies every profile message in all supported languages', () => {
    for (const locale of ['en', 'de', 'es', 'fr', 'it', 'pl'] as const)
      for (const key of profileMessageKeys)
        expect(
          profileText(locale, key, { name: 'Test', count: 2 }),
        ).toBeTruthy();
  });
});
