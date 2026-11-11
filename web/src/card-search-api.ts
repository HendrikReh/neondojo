import { cursorSearchParams } from './cursor-search';
import type { components } from './generated-api';
export const ALL_CARDS = '__all__';
export const CARD_PAGE_SIZE = 50;
export type CardBrowseContext = components['schemas']['CardBrowseContext'];
export type CardFilters = {
  q: string;
  player: string;
  tag: string;
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
export function cardListPath(
  deck: string,
  filters: CardFilters,
  after?: string,
) {
  const params = cursorSearchParams(CARD_PAGE_SIZE, filters, after);
  return `${deck === ALL_CARDS ? '/cards' : `/decks/${encodeURIComponent(deck)}/cards`}?${params}`;
}
