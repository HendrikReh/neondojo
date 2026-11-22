import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { AddCardsPanel } from './LearningPanel';
import { I18nProvider } from './shared/i18n/i18n';

beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());

function render(allowBatch?: boolean) {
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(AddCardsPanel, {
        positionId: 'retained-position',
        matchId: 'collection-host',
        gameNo: 4,
        white: 'Alice',
        black: 'Bob',
        theme: 'forest',
        revision: 0,
        onChanged: () => {},
        allowBatch,
      }),
    }),
  );
}

it('omits unavailable game-error batch controls while keeping current-position cards and deck creation', () => {
  const html = render(false);
  expect(html).toContain('Choose deck');
  expect(html).toContain('Add current position');
  expect(html).toContain('Deck name');
  expect(html).toContain('Create deck');
  expect(html).not.toContain('Minimum loss');
  expect(html).not.toContain('Both players');
  expect(html).not.toContain('Add errors from game 4');
  expect(html).not.toContain('Suggest positions');
});

it('preserves the normal game-error workflow by default', () => {
  const html = render();
  expect(html).toContain('Minimum loss');
  expect(html).toContain('Both players');
  expect(html).toContain('Add errors from game 4');
  expect(html).toContain('Suggest positions');
  expect(html).toBe(render(true));
});
