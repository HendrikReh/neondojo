import { expect, it } from 'vitest';
import {
  ALL_CARDS,
  CARDS_WITHOUT_DECK,
  cardListPath,
  emptyCardFilters,
} from './card-search-api';
import { cardSearchText } from './card-search-messages';
it('encodes literal search characters and carries all filters into subsequent pages', () => {
  const filters = {
    ...emptyCardFilters(),
    q: '  Mochy & 50%_  ',
    tag: 'prime',
    kind: 'checker' as const,
    status: 'reviewed' as const,
  };
  const url = new URL(
    cardListPath(ALL_CARDS, filters, 'cursor+value'),
    'http://local',
  );
  expect(url.pathname).toBe('/cards');
  expect(url.searchParams.get('q')).toBe('Mochy & 50%_');
  expect(url.searchParams.get('tag')).toBe('prime');
  expect(url.searchParams.get('after')).toBe('cursor+value');
  expect(url.searchParams.get('kind')).toBe('checker');
  expect(url.searchParams.get('status')).toBe('reviewed');
  expect(url.searchParams.has('comment')).toBe(false);
  expect(cardListPath('selected', emptyCardFilters())).toBe(
    '/decks/selected/cards?limit=50',
  );
  expect(cardListPath(CARDS_WITHOUT_DECK, emptyCardFilters(), 'next')).toBe(
    '/cards?limit=50&after=next&without_deck=true',
  );
});
it('provides translated card-search labels and localizes result counts in both languages', () => {
  for (const locale of ['en', 'de'] as const) {
    for (const key of [
      'allCards',
      'withoutDeck',
      'search',
      'noResults',
      'kind',
      'status',
      'scopeHint',
    ] as const)
      expect(cardSearchText(locale, key)).not.toBe('');
    expect(cardSearchText(locale, 'found', { count: 1234 })).toContain(
      new Intl.NumberFormat(locale).format(1234),
    );
  }
});
