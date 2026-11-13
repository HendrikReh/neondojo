import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';
import { I18nProvider } from './i18n';
import LearningCardBrowser from './LearningCardBrowser';
import type { CardQuestion } from './learning-api';

beforeEach(() => vi.stubGlobal('localStorage', { getItem: () => 'en' }));
afterEach(() => vi.unstubAllGlobals());
const card = {
  id: 'card-one',
  version: 1,
  tags_version: 0,
  kind: 'checker',
  players: ['Ada', 'Grace'],
  prompt: 'Internal prompt',
  review_count: 3,
  due_at: '2026-11-09T00:00:00Z',
  position: {
    id: 'position',
    match_uuid: 'match',
    ordinal: 1,
    game_no: 2,
    ply: 3,
    player: 0,
    dice: [6, 1],
    board: Array(26).fill(0),
    position_id: 'position-code',
    gnubg_match_id: '',
    notation: 'SECRET PLAYED MOVE',
  },
  source: {
    kind: 'analysis',
    job_id: 'job',
    match_id: 'match',
    position_id: 'position',
    game_no: 2,
    decision_no: null,
  },
} satisfies CardQuestion;
function render(cards: CardQuestion[] = [card]) {
  return renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(LearningCardBrowser, {
        cards,
        theme: 'walnut',
        total: cards.length ? 51 : 0,
        onPageChange: () => {},
      }),
    }),
  );
}
test('the first flashcard has one question-side board preview beside the compact list', () => {
  const html = render();
  expect(html).toContain(
    'class="learning-card-browser position-browser-layout"',
  );
  expect(html).toContain('class="learning-card-list position-browser-main"');
  expect(html).toContain('class="learning-card-preview position-preview"');
  expect(html.match(/class="board-shell /g)).toHaveLength(1);
  expect(html).toContain('theme-walnut');
  expect(html).toContain('data-card-id="card-one"');
  expect(html).toContain('What move would you play?');
  expect(html).toContain('Ada');
  expect(html).toContain('Grace');
  expect(html).toContain('aria-label="Page 2"');
  expect(html).not.toContain('SECRET PLAYED MOVE');
  expect(html).not.toContain('study-answer');
  expect(html).not.toContain('Show answer');
});
test('cube response cards preview the correct question without a checker answer', () => {
  const html = render([{ ...card, kind: 'cube_response' }]);
  expect(html).toContain('Take or pass?');
  expect(html).toContain('Offered cube: 2');
  expect(html).not.toContain('SECRET PLAYED MOVE');
});
test('the list distinguishes cards with the same checker ID at different scores', () => {
  const html = render([
    { ...card, position: { ...card.position, gnubg_match_id: 'MACgAQAAAAAA' } },
    {
      ...card,
      id: 'card-two',
      position: { ...card.position, gnubg_match_id: 'MACgARAAAAAA' },
    },
  ]);
  const list = html.match(/<tbody>(.*?)<\/tbody>/)?.[1];
  expect(list).toContain('0:0 / 13 · Dice 6–1 · Cube 1</small>');
  expect(list).toContain('1:0 / 13 · Dice 6–1 · Cube 1</small>');
});
test('an empty card list cannot display an old board', () => {
  const html = render([]);
  expect(html).not.toContain('class="board-shell ');
  expect(html).toContain('Select a card to preview its position.');
});

test('an inactive workspace retains the list without creating a hidden board', () => {
  const html = renderToStaticMarkup(
    createElement(I18nProvider, {
      children: createElement(LearningCardBrowser, {
        cards: [card],
        theme: 'forest',
        onPageChange: () => {},
        visible: false,
      }),
    }),
  );
  expect(html).toContain('class="learning-card-list position-browser-main"');
  expect(html).not.toContain('class="board-shell ');
});
