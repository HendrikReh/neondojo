import { cursorSearchParams } from '../../../shared/http/cursor-search';
import type { components } from '../../../generated-api';
export const ALL_CARDS = '__all__';
export const CARDS_WITHOUT_DECK = '__without_deck__';
export const CARD_SELECTION_LIMIT = 200;
export const CARD_PAGE_SIZE = 50;
export const isCollectionScope = (id: string) =>
  id === ALL_CARDS || id === CARDS_WITHOUT_DECK;
export type CardBrowseContext = components['schemas']['CardBrowseContext'];
export type CardFilters = {
  q: string;
  player: string;
  tag: string;
  topics?: string;
  game_plan?: '' | components['schemas']['GamePlan'];
  comment: string;
  position_id: string;
  kind: '' | components['schemas']['CardKind'];
  status: '' | components['schemas']['CardReviewState'];
};
export const emptyCardFilters = (): CardFilters => ({
  q: '',
  player: '',
  tag: '',
  comment: '',
  position_id: '',
  kind: '',
  status: '',
});
export function normalizeCardFilters(filters: CardFilters): CardFilters {
  const normalized = { ...filters };
  for (const key of Object.keys(normalized) as (keyof CardFilters)[])
    normalized[key] = normalized[key]?.trim() as never;
  normalized.tag = normalized.tag.toLowerCase();
  return normalized;
}
export function cardListPath(
  deck: string,
  filters: CardFilters,
  page?: number | string,
) {
  const params = cardFilterParams(
    CARD_PAGE_SIZE,
    filters,
    typeof page === 'string' ? page : undefined,
  );
  if (typeof page === 'number') params.set('page', String(page));
  if (deck === CARDS_WITHOUT_DECK) params.set('without_deck', 'true');
  return `${isCollectionScope(deck) ? '/cards' : `/decks/${encodeURIComponent(deck)}/cards`}?${params}`;
}

export function cardPracticePath(
  deck: string,
  filters: CardFilters,
  limit: number,
) {
  const params = cardFilterParams(limit, filters);
  if (deck === CARDS_WITHOUT_DECK) params.set('without_deck', 'true');
  else if (!isCollectionScope(deck)) params.set('deck_id', deck);
  return `/study/card-rounds?${params}`;
}

function cardFilterParams(
  limit: number,
  { topics = '', game_plan = '', ...filters }: CardFilters,
  after?: string,
) {
  return cursorSearchParams(limit, { ...filters, topics, game_plan }, after);
}
