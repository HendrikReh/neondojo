import { expect, test } from 'vitest';
import { canEnterRating } from './PlayerSummary';

test('only imports and unassisted Competition bot matches can enter the PR', () => {
  expect(canEnterRating({ play_mode: null, play_assisted: false })).toBe(true);
  expect(
    canEnterRating({ play_mode: 'competition', play_assisted: false }),
  ).toBe(true);
  expect(
    canEnterRating({ play_mode: 'competition', play_assisted: true }),
  ).toBe(false);
  for (const mode of ['training', 'coach', 'tutor'] as const)
    expect(canEnterRating({ play_mode: mode, play_assisted: true })).toBe(
      false,
    );
});
