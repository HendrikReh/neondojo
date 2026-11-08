import { expect, it } from 'vitest';
import { annotationMessages, annotationText } from './annotation-messages';
import { learningMessages } from './learning-messages';

it('retains every dynamic message argument in each annotation and learning language', () => {
  for (const messages of [annotationMessages, learningMessages]) {
    const source = messages.en as Record<string, string>;
    for (const catalog of Object.values(messages)) {
      expect(Object.keys(catalog).sort()).toEqual(Object.keys(source).sort());
      for (const [key, text] of Object.entries(
        catalog as Record<string, string>,
      )) {
        const args = (value: string) =>
          [
            ...new Set([...value.matchAll(/\{(\w+)\}/g)].map((m) => m[1])),
          ].sort();
        expect(args(text), key).toEqual(args(source[key]));
      }
    }
  }
  expect(annotationText('de', 'commentCount', { count: 1234 })).toContain(
    '1.234',
  );
  expect(annotationText('fr', 'removeTag', { tag: '<script>' })).toContain(
    '<script>',
  );
});
