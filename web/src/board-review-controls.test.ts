import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import BoardReviewControls from './BoardReviewControls';
import { I18nProvider } from './i18n';
import type { PreviewCandidate } from './candidate-preview';

beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());

const render = (selected?: PreviewCandidate) =>
  renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(BoardReviewControls, {
        view: 'arrows',
        preview: { ok: true, board: [], steps: [], forced: false },
        selected,
        onView: () => {},
        onPlayed: () => {},
      }),
    }),
  );

it('uses concise board labels without repeating the played move', () => {
  const html = render();
  for (const label of ['Initial', 'Arrows', 'Final'])
    expect(html).toContain(`>${label}</button>`);
  expect(html).not.toContain('board-review-selection');
  expect(html).not.toContain('8/5 6/5');
});

it('keeps candidate provenance and an explicit preview reset', () => {
  const html = render({
    rank: 2,
    notation: '13/10 6/5',
    source: 'GNUbg 2-ply',
  });
  expect(html).toContain('Candidate 2');
  expect(html).toContain('GNUbg 2-ply');
  expect(html).toContain('Reset preview');
  expect(html).not.toContain('Played move');
});

it('localizes the arrows label for German board navigation', () => {
  vi.stubGlobal('localStorage', { getItem: () => 'de' });
  const html = render();
  expect(html).toContain('>Pfeile</button>');
  expect(html).toContain('>Initial</button>');
  expect(html).toContain('>Final</button>');
});
