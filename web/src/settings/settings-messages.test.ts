import { expect, it } from 'vitest';
import { settingsMessages, settingsText } from './settings-messages';

it('provides every settings label and dynamic argument in both languages', () => {
  expect(Object.keys(settingsMessages).sort()).toEqual(['de', 'en']);
  const source = settingsMessages.en as Record<string, string>;
  const placeholders = (value: string) =>
    [...value.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
  for (const catalog of Object.values(settingsMessages)) {
    expect(Object.keys(catalog).sort()).toEqual(Object.keys(source).sort());
    for (const [key, value] of Object.entries(catalog)) {
      expect(value.trim().length, key).toBeGreaterThan(0);
      expect(placeholders(value), key).toEqual(placeholders(source[key]));
    }
  }
  expect(
    settingsText('de', 'checkSuccess', { version: 'GNUbg 1.08' }),
  ).toContain('GNUbg 1.08');
  expect(settingsText('en', 'preview', { theme: 'Rain' })).toContain('Rain');
});
