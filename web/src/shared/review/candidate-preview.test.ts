import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { I18nProvider } from '../i18n/i18n';
import SourceAnalysisView from '../../workspaces/analysis/candidates/SourceAnalysisView';
import type { BookLookup } from '../../workspaces/analysis/candidates/source-analysis-api';
import { candidatePreviewNotation } from './candidate-preview';

test('uses the recorded steps in preference to compact notation', () => {
  expect(
    candidatePreviewNotation({ notation: '24/15', path: '24/21 21/15' }),
  ).toBe('24/21 21/15');
  expect(candidatePreviewNotation({ notation: '24/15' })).toBe('24/15');
});

beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const book: BookLookup = {
  provider: 'backgammondb',
  hit: true,
  cube: null,
  checker: {
    plies: 3,
    candidates: [
      {
        rank: 1,
        notation: '8/5 6/5',
        equity: 0.2,
        probabilities: [0.55, 0.1, 0.01, 0.45, 0.08, 0.01],
      },
      {
        rank: 2,
        notation: '13/10 6/5',
        equity: 0.1,
        probabilities: [0.52, 0.1, 0.01, 0.48, 0.08, 0.01],
      },
    ],
  },
};
function render(onPreview?: () => void) {
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(SourceAnalysisView, {
        records: [],
        book,
        source: 'book',
        onPreview,
      }),
    }),
  );
}
test('ranked checker candidates expose explicit accessible board preview actions', () => {
  const html = render(() => {});
  expect(html).toContain('aria-label="Preview move 1: 8/5 6/5"');
  expect(html).toContain('aria-label="Preview move 2: 13/10 6/5"');
  expect(html).not.toContain('Preview best move');
  expect(html.match(/candidate-preview-row/g)).toHaveLength(2);
});
test('a source view without a preview handler remains read-only', () => {
  expect(render()).not.toContain('Preview move 1:');
  expect(render()).not.toContain('candidate-preview-row');
});
