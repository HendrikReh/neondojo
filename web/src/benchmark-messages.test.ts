import { expect, test } from 'vitest';
import { benchmarkText } from './benchmark-messages';

test('play latency describes the selected engine in German and English', () => {
  expect(benchmarkText('de', 'playTiming')).toContain('gewählten Engine');
  expect(benchmarkText('en', 'playTiming')).toContain('selected engine');
});
