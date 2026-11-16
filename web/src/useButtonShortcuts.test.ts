import { describe, expect, it } from 'vitest';
import { shortcutToken } from './useButtonShortcuts';

const key = (key: string, modifiers: Partial<KeyboardEventInit> = {}) => ({
  key,
  shiftKey: false,
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  ...modifiers,
});

describe('shortcut tokens', () => {
  it('names letters, digits and Enter the way buttons declare them', () => {
    expect(shortcutToken(key('d'))).toBe('D');
    expect(shortcutToken(key('3'))).toBe('3');
    expect(shortcutToken(key('Enter'))).toBe('Enter');
    expect(shortcutToken(key('E', { shiftKey: true }))).toBe('Shift+E');
  });
  it('leaves browser and system shortcuts alone', () => {
    expect(shortcutToken(key('d', { metaKey: true }))).toBeUndefined();
    expect(shortcutToken(key('1', { ctrlKey: true }))).toBeUndefined();
    expect(shortcutToken(key('e', { altKey: true }))).toBeUndefined();
    expect(shortcutToken(key('ArrowLeft'))).toBeUndefined();
    expect(shortcutToken(key(' '))).toBeUndefined();
  });
});
