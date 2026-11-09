import { expect, it } from 'vitest';
import { deckManagementMessages } from './deck-management-messages';
it('provides the same deck dialog messages and interpolation values in all six languages', () => {
  const source = deckManagementMessages.en;
  const placeholders = (value: string) =>
    [...value.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();
  expect(Object.keys(deckManagementMessages)).toHaveLength(6);
  for (const catalog of Object.values(deckManagementMessages)) {
    expect(Object.keys(catalog).sort()).toEqual(Object.keys(source).sort());
    for (const key of Object.keys(source) as (keyof typeof source)[]) {
      expect(catalog[key]).not.toBe('');
      expect(placeholders(catalog[key]), key).toEqual(
        placeholders(source[key]),
      );
    }
  }
});
