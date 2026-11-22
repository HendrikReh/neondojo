import { request } from '../../shared/http/api';
import type { components } from '../../generated-api';

export type PositionBookmark = Required<
  components['schemas']['PositionBookmark']
>;
export function loadPositionBookmark(id: string, signal?: AbortSignal) {
  return request<PositionBookmark>(
    `/positions/${encodeURIComponent(id)}/bookmark`,
    {
      signal,
    },
  );
}
export function savePositionBookmark(id: string, signal?: AbortSignal) {
  return request<PositionBookmark>(
    `/positions/${encodeURIComponent(id)}/bookmark`,
    {
      method: 'PUT',
      signal,
    },
  );
}
export function removePositionBookmark(id: string, signal?: AbortSignal) {
  return request<void>(`/positions/${encodeURIComponent(id)}/bookmark`, {
    method: 'DELETE',
    signal,
  });
}
