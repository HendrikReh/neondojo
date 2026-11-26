import { describe, expect, it } from 'vitest';
import { playTextCatalogs } from './play-messages';
import { playModeTextCatalogs } from './play-mode-messages';

const placeholders = (text: string) =>
  [...text.matchAll(/\{(\w+)/g)].map((m) => m[1]).sort();

describe.each([
  ['play-messages', playTextCatalogs],
  ['play-mode-messages', playModeTextCatalogs],
])('%s', (_, catalogs) => {
  const { en, ...others } = catalogs as Record<string, Record<string, string>>;
  it('translates every English text into every language', () => {
    expect(Object.keys(others).sort()).toEqual(['de']);
    const missing = Object.entries(others).flatMap(([locale, texts]) =>
      Object.keys(en)
        .filter((key) => !texts[key]?.trim())
        .map((key) => `${locale}.${key}`),
    );
    expect(missing).toEqual([]);
  });
  it('keeps the English placeholders and no unknown keys', () => {
    const problems = Object.entries(others).flatMap(([locale, texts]) =>
      Object.entries(texts).flatMap(([key, text]) =>
        !(key in en)
          ? [`${locale}.${key} unknown`]
          : JSON.stringify(placeholders(text)) !==
              JSON.stringify(placeholders(en[key]))
            ? [`${locale}.${key} placeholders`]
            : [],
      ),
    );
    expect(problems).toEqual([]);
  });
});
