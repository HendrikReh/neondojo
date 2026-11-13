import { request } from './api';
import { AppError } from './i18n-core';

import type { components } from './generated-api';
type Schema = components['schemas'];
export type AnnotationKind = Schema['AnnotationKind'];
export type Annotation = Required<Schema['Annotation']>;
export type AnnotationHit = Omit<Schema['AnnotationHit'], 'target_kind'> & {
  target_kind: AnnotationKind;
};
export type AnnotationSearchPage = Required<Schema['AnnotationPage']>;

export interface AnnotationFilters {
  q: string;
  tag: string;
  kind: AnnotationKind | '';
  database_id?: string;
}

export type AnnotationValidation =
  | 'invalidTag'
  | 'tagTooLong'
  | 'tooManyTags'
  | 'commentTooLong'
  | 'invalidComment';

export type SearchValidation =
  | AnnotationValidation
  | 'searchRequired'
  | 'queryTooShort'
  | 'queryTooLong'
  | 'invalidQuery';

export const normalizeAnnotationTag = (tag: string) =>
  tag
    .trim()
    .replace(/\p{White_Space}+/gu, ' ')
    .toLowerCase();

export function normalizedAnnotationTags(tags: string[]) {
  return [...new Set(tags.map(normalizeAnnotationTag).filter(Boolean))].sort();
}

export function annotationValidation(
  tags: string[],
  comment: string,
): AnnotationValidation | null {
  if (tags.some((tag) => /\p{Cc}/u.test(tag))) return 'invalidTag';
  const normalized = normalizedAnnotationTags(tags);
  if (normalized.some((tag) => [...tag].length > 64)) return 'tagTooLong';
  if (normalized.length > 30) return 'tooManyTags';
  if ([...comment].length > 20_000) return 'commentTooLong';
  if (/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/u.test(comment))
    return 'invalidComment';
  return null;
}

export function searchValidation(
  q: string,
  tag: string,
): SearchValidation | null {
  if (/\p{Cc}/u.test(q)) return 'invalidQuery';
  const queryLength = [...q.trim()].length;
  if (!queryLength && !normalizeAnnotationTag(tag)) return 'searchRequired';
  if (queryLength && queryLength < 3) return 'queryTooShort';
  if (queryLength > 200) return 'queryTooLong';
  return annotationValidation(tag ? [tag] : [], '');
}

export const annotationPath = (kind: AnnotationKind, id: string) =>
  `/${({ match: 'matches', position: 'positions', game: 'games', move: 'moves' } as const)[kind]}/${encodeURIComponent(id)}/annotation`;

export const isAnnotationConflict = (error: unknown) =>
  error instanceof AppError && String(error.params.status) === '409';

export function searchAnnotations(
  filters: AnnotationFilters,
  page: number,
  signal: AbortSignal,
) {
  const params = new URLSearchParams({ limit: '20', page: String(page) });
  if (filters.q) params.set('q', filters.q);
  if (filters.tag) params.set('tag', filters.tag);
  if (filters.kind) params.set('kind', filters.kind);
  if (filters.database_id) params.set('database_id', filters.database_id);
  return request<AnnotationSearchPage>(`/annotations/search?${params}`, {
    signal,
  });
}
