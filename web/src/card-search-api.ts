import type { components } from './generated-api';
export const ALL_CARDS = '__all__';
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
  const params = new URLSearchParams({ limit: '50' });
  for (const [key, value] of Object.entries(filters))
    if (value.trim()) params.set(key, value.trim());
  if (after) params.set('after', after);
  return `${deck === ALL_CARDS ? '/cards' : `/decks/${encodeURIComponent(deck)}/cards`}?${params}`;
}
