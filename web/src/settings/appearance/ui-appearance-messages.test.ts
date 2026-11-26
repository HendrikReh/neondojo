import { expect, it } from 'vitest';
import {
  uiAppearanceMessages,
  uiAppearanceText,
} from './ui-appearance-messages';

it('provides complete appearance labels for both app languages', () => {
  expect(Object.keys(uiAppearanceMessages).sort()).toEqual(['de', 'en']);
  const source = uiAppearanceMessages.en;
  for (const messages of Object.values(uiAppearanceMessages)) {
    expect(Object.keys(messages).sort()).toEqual(Object.keys(source).sort());
    for (const value of Object.values(messages))
      expect(value.trim().length).toBeGreaterThan(0);
  }
  expect(uiAppearanceText('de', 'appearance')).toBe('Darstellung');
  expect(uiAppearanceText('en', 'system')).toBe('System');
});
